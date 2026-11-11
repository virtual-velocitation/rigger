//! The default context-graph projector: it folds the event log into bi-temporal
//! node and edge tables in a local SQLite file and answers Subgraph and Resolve.
//! A single connection behind a mutex serializes the read-then-write of apply.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};

use super::{
    Edge, Error, FoldAccess, Graph, Neighbors, Node, RecordFold, KIND_ARCH_DECISION, KIND_ARTIFACT,
    KIND_CODE_ENTITY, KIND_DECISION, KIND_DESIGN_DOC, KIND_FILE, KIND_FINDING, KIND_HANDBOOK_RULE,
    KIND_LESSON, KIND_RATIONALE, OWED_LOST_FOLD, OWED_OLDER_RULE, PLACEHOLDER_SIGIL, REBUILD_OWED,
    REBUILD_OWED_FOR_LOST_FOLD, REL_ABOUT, REL_GOVERNS, REL_RAISED, REL_SUPERSEDES, TIER_EXTRACTED,
    TYPE_ALIAS_DEFINED, TYPE_ALIAS_UNRESOLVED, TYPE_DECISION_MADE, TYPE_FILE_TOUCHED,
    TYPE_GATE_VERDICT, TYPE_LESSON_LEARNED, TYPE_REVIEW_FINDING, TYPE_UNIT_INTEGRATED,
    TYPE_UNIT_STARTED,
};
use crate::eventstore::{from_nanos, to_nanos, Event, EventStore, Filter, Position, Revision};
use crate::lockfile::HeldLock;
use crate::spawn::{SpawnEvent, SpawnResult, TYPE_SPAWN_RESULT};
use crate::sqlite::open_connection;
use rigger_domain::retention::DERIVED_TYPES;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS nodes (
  id TEXT NOT NULL, kind TEXT NOT NULL, attrs TEXT,
  project TEXT NOT NULL DEFAULT '',
  PRIMARY KEY (id, project)
);
CREATE TABLE IF NOT EXISTS edges (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  from_id TEXT NOT NULL, to_id TEXT NOT NULL, rel TEXT NOT NULL,
  valid_from INTEGER NOT NULL, valid_to INTEGER, source INTEGER NOT NULL,
  project TEXT NOT NULL DEFAULT '',
  tier TEXT NOT NULL DEFAULT 'extracted'
);
CREATE INDEX IF NOT EXISTS idx_edges_from ON edges(from_id);
CREATE INDEX IF NOT EXISTS idx_edges_to ON edges(to_id);
CREATE TABLE IF NOT EXISTS aliases (alias TEXT PRIMARY KEY, canonical_id TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS applied (position INTEGER PRIMARY KEY);
CREATE TABLE IF NOT EXISTS node_assertions (
  project TEXT NOT NULL, identity TEXT NOT NULL, generation TEXT NOT NULL, node_id TEXT NOT NULL,
  kind TEXT NOT NULL, attrs TEXT,
  PRIMARY KEY (project, identity, node_id)
);
CREATE INDEX IF NOT EXISTS idx_node_assertions_node ON node_assertions(project, node_id);
CREATE TABLE IF NOT EXISTS passed_over (position INTEGER PRIMARY KEY, cause TEXT NOT NULL);
";

/// The fold rule `graph.db` was folded under, recorded as the file's `user_version`. Rule 1 is
/// generations superseding their predecessors (spec 101): a file folded before it holds facts a
/// superseded generation asserted and has no assertion ledgers to retire them from. Rule 2 is
/// the shared node's one kind (spec 116): a file folded before it gave
/// a node the definitions of one file share the kind and line of the last one folded. Rule 3 is
/// the node addressed by its path (spec 119): a file folded before it holds a nested definition
/// at its flat `<file>::<leaf>` id rather than its path's, with no `CONTAINS` from its parent.
/// Rule 4 is a record's reference read as the id it names
/// ([`referenced_id`](super::query::referenced_id)): a file folded before it holds a record citing
/// lines of a file (`path:line`) ABOUT or GOVERNING an artifact at the citation rather than the
/// file. Rule 5 is a lesson's or finding's `file` read as one more location
/// ([`locations`](super::locations)): a file folded before it holds a record naming its location
/// by `file` with no ABOUT edge to that file. Rule 6 is a community membership held only by code
/// ([`holds_attachment`]): a file folded before it keeps an `IN_COMMUNITY` edge live on a path whose
/// code retired while a record still names it. Rule 7 is the records-only fold: every derived type
/// is skipped, so a file folded before it holds the code, document and grouping facts the code
/// store's view now answers, and its rebuild swaps in only a refold that keeps every record fact it
/// holds ([`refuse_a_lossy_refold`]). A file at an older version that folded anything owes
/// one cold rebuild from the log ([`Projector::rebuild`], which only `rigger setup` runs) and
/// nothing folds into it incrementally until then. A rebuild's shadow records the rule it is
/// folded under the same way, from the moment it is opened, so one a binary of another rule left
/// is never resumed ([`open_shadow`]).
pub const PROJECTION_VERSION: i64 = 7;

/// The assertion ledgers, dropped from a file at an older [`PROJECTION_VERSION`] so [`SCHEMA`]
/// recreates them in their current shape, and every table and view an older rule kept for the code
/// fold, which no rule since holds.
const DROP_LEDGERS: &str = "
DROP VIEW IF EXISTS live_node_assertions;
DROP VIEW IF EXISTS live_edge_assertions;
DROP TABLE IF EXISTS generations;
DROP TABLE IF EXISTS node_assertions;
DROP TABLE IF EXISTS edge_assertions;
DROP TABLE IF EXISTS retired_nodes;
DROP TABLE IF EXISTS detached_attachments;
DROP TABLE IF EXISTS relabel_owed;
DROP TABLE IF EXISTS refs;
DROP TABLE IF EXISTS proofs_owed;
DROP TABLE IF EXISTS pending_proof;
DROP TABLE IF EXISTS proofs;
";

/// Every table of [`SCHEMA`] beside `nodes` and `edges` that carries a `project` column: the
/// rows [`Projector::migrate_project`] re-keys with the facts, each only when the file holds it
/// with that column, since a file that owes its rebuild is opened as it stands and may predate
/// any of them.
const PROJECT_KEYED_LEDGERS: [&str; 1] = ["node_assertions"];

/// The state a rebuild ([`Projector::rebuild`]) keeps in the shadow file it folds and, from the
/// swap until the tail it owes is folded, in the live one, born together in one transaction
/// ([`in_transaction`]) and dropped together in one ([`DROP_REBUILD_STATE`]) once the tail is
/// folded - never apart:
/// - `rebuild_cursor`: the last position the rebuild folded, and what its run-closure prune removed
///   ([`prune_run_closure`]) - zero in the shadow, which is never pruned, and the one prune's counts
///   in the pruned copy the swap puts in place of the live file, so a rebuild interrupted in its
///   tail still reports what that prune removed ([`Rebuilt::pruned`]).
/// - `rebuild_run_closure`: the run attribution it gathers as it folds (spec 101) - every folded
///   event of the [`rigger_domain::run::RUN_CLOSURE_TYPES`], committed with the transaction that
///   folded it - so every pass that reaches the swap, a resumed one included, derives the
///   run-closure prune from the whole of it ([`prune_run_closure`]).
const REBUILD_STATE: &str = "
CREATE TABLE IF NOT EXISTS rebuild_cursor (
    position INTEGER NOT NULL,
    pruned_nodes INTEGER NOT NULL DEFAULT 0,
    reclaimed_edges INTEGER NOT NULL DEFAULT 0
);
INSERT INTO rebuild_cursor (position) SELECT 0 WHERE NOT EXISTS (SELECT 1 FROM rebuild_cursor);
CREATE TABLE IF NOT EXISTS rebuild_run_closure (
    position INTEGER PRIMARY KEY,
    type TEXT NOT NULL,
    data BLOB NOT NULL,
    valid_from INTEGER NOT NULL
);
";

/// The name of the one-row cursor table saying how far the file's prefix of the log reaches (spec
/// 124, THE CURSOR AND ITS SEEDING): what its creation ([`create_cursor`]), every read of it
/// ([`cursor_row`]) and every move of it ([`set_cursor`]) name it by.
const CAUGHT_UP: &str = "caught_up";

/// The bound [`CATCH_UP_HORIZON`] adds its margin to (spec 124, THE CLOCKS): ten minutes, the
/// longest an appender ([`Projector::guarded`]) waits, counted from its lowest own position's
/// stamp, for its positions to fold - by its calls, or by its direct fold's retried opens past
/// another writer's write lock - before it returns them not folded, the gap left in the ledger.
const FOLD_LOCK_CEILING: Duration = Duration::from_secs(600);

/// How old a position missing from the `applied` ledger must be before a seeding of the
/// `caught_up` cursor ([`Projector::seed_cursor`]) counts it lost (spec 124, THE CLOCKS): the
/// [`FOLD_LOCK_CEILING`] plus a margin of 60 s. The ceiling is the longest an appender
/// ([`Projector::guarded`]) waits for its positions to fold, and its direct fold opens no later
/// than its stamp plus the ceiling plus one busy timeout of its connection, which the margin
/// covers, so a younger missing position may still be folded by its appender.
pub const CATCH_UP_HORIZON: Duration = Duration::from_secs(FOLD_LOCK_CEILING.as_secs() + 60);

/// Whether `stamp` is older than `bound` at `now` (spec 124, THE CLOCKS), the one age rule a
/// seeding's horizon ([`Projector::seed_cursor`]) and an appender's ceiling
/// ([`Projector::past_ceiling`]) read: an absent stamp counts old, one more than `bound` before
/// `now` old, and any other - one after `now` included - young.
fn older_than(stamp: Option<SystemTime>, bound: Duration, now: SystemTime) -> bool {
    stamp.is_none_or(|stamp| now.duration_since(stamp).is_ok_and(|age| age > bound))
}

/// The ceiling `held` as a cause text spells it (spec 124, CAUSE TEXTS): in whole minutes at or
/// above one minute, else in whole seconds, each rounded down.
fn held_for(held: Duration) -> String {
    match held.as_secs() {
        secs if secs >= 60 => format!("{} min", secs / 60),
        secs => format!("{secs} s"),
    }
}

/// The suffix a call-run fold's not-folded text ends with (spec 124, CAUSE TEXTS): an appender
/// whose positions its calls ([`Projector::catch_up`]) had not folded leaves them to the next.
const CALL_PAYS: &str = " - the next fold through the folding store catches it up";

/// The suffix a direct fold's not-folded text ends with (spec 124, CAUSE TEXTS), for own events all
/// of the project's run stream ([`on_the_run_stream`]): `rigger setup` reads that stream's
/// positions against the file's `applied` ledger and folds the one missing there.
const DIRECT_PAYS: &str =
    " - the next rigger setup finds the event missing from graph.db and folds it";

/// The not-folded text of an appender whose calls met the file's rebuild state (spec 124, CAUSE
/// TEXTS): past its ceiling, or at once when its own process holds the file's rebuild lock.
const UNFINISHED_REBUILD: &str = "graph.db holds an unfinished rebuild";

/// Why an appender's positions stood short of the `caught_up` cursor when its ceiling passed
/// (spec 124, CAUSE TEXTS): what its text says held them off ([`Projector::past_the_ceiling`]).
enum Short {
    /// Another writer's write lock held its fold's open off.
    Contended,
    /// Its calls had not reached its positions.
    CatchingUp,
}

/// Drop the [`REBUILD_STATE`] a rebuild kept, both tables in one transaction ([`in_transaction`]).
const DROP_REBUILD_STATE: &str = "
DROP TABLE rebuild_cursor;
DROP TABLE rebuild_run_closure;
";

/// Where a [`RebuildSource`] hands each batch it streams, with the log's last position.
pub type RebuildSink<'s> = dyn FnMut(&[Event], Position) -> Result<(), Error> + 's;

/// Streams the log's live selection after a position into a [`RebuildSink`], in batches, each
/// handed with the log's last position so a caller can say how far along it is (spec 101): what a
/// [`Projector::rebuild`] folds. The sqlite event store's `read_live_selection` is the one in
/// production.
pub type RebuildSource<'s> = dyn FnMut(Position, &mut RebuildSink) -> Result<(), Error> + 's;

/// Hand `sink` the events of `log` - in position order - past position `after`, in batches of at
/// most `batch`, each with the log's last position: the [`RebuildSource`] body for a log already
/// read into memory.
pub fn stream_past(
    log: &[Event],
    after: Position,
    batch: usize,
    sink: &mut RebuildSink,
) -> Result<(), Error> {
    let head = log.last().map_or(after, |e| e.position);
    let from = log.partition_point(|e| e.position <= after);
    log[from..].chunks(batch).try_for_each(|b| sink(b, head))
}

/// Where a [`PositionSource`] hands each batch of positions it streams.
pub type PositionSink<'s> = dyn FnMut(&[Position]) -> Result<(), Error> + 's;

/// Streams the positions of the log's live selection into a [`PositionSink`], in position order,
/// in batches - positions alone, never the events: what [`Projector::seed_cursor`] reads a
/// graph's ledger against (spec 101, spec 124). The sqlite event store's `read_live_positions` is
/// the one in production.
pub type PositionSource<'s> = dyn FnMut(&mut PositionSink) -> Result<(), Error> + 's;

/// The store stamp of the event the log holds at a position, read by one point lookup: what
/// [`Projector::seed_cursor`] reads a missing position's age through (spec 124, THE CLOCKS). None
/// is no event at that position, or no stamp on the folding host's clock. The sqlite event store's
/// `recorded_at` is the one in production on the embedded log, whose stamp and the folding host's
/// clock are one clock; the server-backed log's stamp is taken on another host's clock, so
/// `rigger setup` hands the seeding one answering none there: the server's skew moves nothing, and
/// every position the file misses counts old.
pub type StampLookup<'s> = dyn FnMut(Position) -> Result<Option<SystemTime>, Error> + 's;

/// The [`PositionSource`] over `stream` of `store`, for a store with no live selection (a
/// server-backed log, whose live selection is its stream as it stands): the positions the store
/// port reads in one ordered pass, positions alone ([`EventStore::read_stream_positions`]), in
/// batches of at most `batch`.
pub fn stream_positions<'s>(
    store: &'s dyn EventStore,
    stream: &'s str,
    batch: usize,
) -> impl FnMut(&mut PositionSink) -> Result<(), Error> + 's {
    move |sink| {
        store
            .read_stream_positions(stream, batch, &mut |positions| {
                sink(positions).map_err(|e| crate::eventstore::Error::Backend(e.0))
            })
            .map_err(be)
    }
}

/// The [`RebuildSource`] over `stream` of `store`, for a store with no live selection: each call
/// reads only what the stream gained since the call before it - the first from its start - through
/// the store's batched read ([`EventStore::read_stream_batched`]), holding at most one batch of
/// `batch` events at a time, and hands on the events of each batch past `after`, so a rebuild's
/// fold and its tail read each event of the stream once between them and never hold the stream.
pub fn stream_source<'s>(
    store: &'s dyn EventStore,
    stream: &'s str,
    batch: usize,
) -> impl FnMut(Position, &mut RebuildSink) -> Result<(), Error> + 's {
    let mut next: Revision = 0;
    move |after, sink| {
        store
            .read_stream_batched(stream, next, batch, &mut |events, head| {
                next = events.last().map_or(next, |e| e.revision + 1);
                // The batch's events past `after`, handed with the stream's head.
                stream_past(events, after, batch, &mut |past, _| sink(past, head))
                    .map_err(|e| crate::eventstore::Error::Backend(e.0))
            })
            .map_err(be)
    }
}

/// How far a [`Projector::rebuild`] has folded: `folded` events so far, the last through position
/// `through`, of the log from `start` (exclusive) to `head`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RebuildProgress {
    pub start: Position,
    pub through: Position,
    pub head: Position,
    pub folded: usize,
}

/// Why a command that needs the rebuild lock ([`Projector::lock_rebuild`]) is refused at once:
/// another holds it - a `rigger setup` rebuilding `graph.db`, a `rigger reset --runs` removing a
/// stale pruned copy, or a `rigger reset --derived` compacting the log. The one spelling of that
/// refusal, true of every holder, which `rigger setup` and `rigger reset --derived` exit on and
/// `rigger reset --runs` reports.
pub const REBUILD_IN_PROGRESS: &str =
    "graph.db.lock is held by another `rigger setup` or `rigger reset`";

/// The exclusion a [`Projector::rebuild`] of one graph file holds, from before it opens or creates
/// its shadow until its tail is folded and its cursor dropped (spec 101), and that a `rigger reset`
/// holds while it removes a stale pruned copy or compacts the log: the OS advisory lock on the
/// zero-byte `<path>.lock` beside the graph file - a file no holder removes, and one SQLite never
/// opens, so no connection's own locks ride on it. Only a rebuild holding it ever has a shadow
/// open, so a shadow is removed only when no other connection holds it. The lock is released when
/// this is dropped - explicitly, never by closing its descriptor alone ([`HeldLock`]) - and by the
/// OS when its process ends however it ends, so an interrupted holder leaves no stale lock.
#[derive(Debug)]
pub struct RebuildLock {
    /// The graph file the rebuild is of.
    path: String,
    /// The held lock on `<path>.lock`, released when this drops.
    _locked: HeldLock,
}

impl RebuildLock {
    /// The graph file this lock excludes other rebuilds of.
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// What [`Projector::forget_stale_copy`] found beside a graph file, and what it did with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaleCopy {
    /// No pruned copy stood there.
    Absent,
    /// A stale copy stood there, and it is removed.
    Removed,
    /// A copy stood there while another held the rebuild lock - a rebuild, whose own copy it may
    /// be, or a `rigger reset`: kept.
    InUse,
}

/// Projector is the SQLite-backed RecordFold.
///
/// `project` is the plain project string `Namespaced::new` uses to build the `proj-<id>-`
/// stream prefix (spec 28): every node and edge this projector folds is stamped with it, so a
/// single shared backend can hold many projects without their nodes/edges ever mixing. It is
/// injected at construction, never derived a second way - the SAME identity that namespaces
/// the streams is the ONE source of truth for project scope on the graph.
pub struct Projector {
    conn: Mutex<Connection>,
    project: String,
    /// The graph file this projector was opened on, the one [`Projector::rebuild`] rebuilds.
    path: String,
    /// Where an older binary's failed fold left its mark beside this file ([`owed_mark`]): no cause
    /// of its rebuild, only what setup and the rebuild's swap remove.
    owed_mark: Option<PathBuf>,
    /// The event log [`Projector::catch_up`] folds this file forward from, once installed
    /// ([`Projector::with_log`]): none folds nothing forward.
    log: Option<CatchUpLog>,
    /// The identity of the event log this projector folds ([`Projector::with_store_identity`]):
    /// what the seeding and a rebuild record beside the `caught_up` cursor they create, and what
    /// the seeding and every catch-up compare the cursor's with ([`log_switched`]); empty for a
    /// projector told none.
    store: String,
    /// The longest an appender in [`Projector::guarded`] waits for its own positions to fold
    /// (spec 124, THE APPENDER): [`FOLD_LOCK_CEILING`], unless a test injects another
    /// ([`Projector::with_lock_ceiling`]).
    lock_ceiling: Duration,
    /// Whether this projector's process holds the rebuild lock of its file
    /// ([`Projector::with_rebuild_lock`], which states what its appender answers then).
    holds_rebuild_lock: bool,
}

/// The event log a [`Projector::catch_up`] folds forward from: the opener installed
/// ([`Projector::with_log`]) and the log it opened, kept once an open succeeds so it is opened
/// once for the projector's life; a failed open keeps nothing, so the next call opens again.
struct CatchUpLog {
    open: Box<dyn Fn() -> Result<Arc<dyn EventStore>, Error> + Send + Sync>,
    opened: Mutex<Option<Arc<dyn EventStore>>>,
}

impl CatchUpLog {
    /// The log, opened by the first call that asks it and kept: the kept one when an open has
    /// succeeded, else what the opener answers now, kept when it is the log. The cache's lock is
    /// released as this returns, so no lock of it is held across a read of the log.
    fn opened(&self) -> Result<Arc<dyn EventStore>, Error> {
        let mut opened = self.opened.lock().unwrap();
        if let Some(log) = &*opened {
            return Ok(Arc::clone(log));
        }
        let log = (self.open)()?;
        *opened = Some(Arc::clone(&log));
        Ok(log)
    }
}

/// The file an older binary wrote beside the graph file at `path` when a fold into it failed - a
/// write lost to another writer's lock left the file current but missing an event; no fold writes
/// it now, and it is no cause of a rebuild (spec 124, THE MARK): the debt is the position missing
/// from the file's `applied` ledger, which `rigger setup`'s seeding of a file not yet carrying the
/// `caught_up` cursor reads against the log up to the bound [`Projector::seed_cursor`] names. It is
/// only removed: by setup once the file carries `caught_up` ([`Projector::forget_cursored_mark`]),
/// by the rebuild's swap ([`Projector::rebuild`]), and once the file is gone
/// ([`Projector::forget_orphaned_mark`]). `None` for an in-memory graph, which no rebuild outlives.
fn owed_mark(path: &str) -> Option<PathBuf> {
    (path != ":memory:").then(|| PathBuf::from(format!("{path}.owed")))
}

/// Why the graph file behind `conn` says it owes its rebuild: the older fold rule it records
/// ([`OWED_OLDER_RULE`]), or no cause - the [`owed_mark`] beside it is none (spec 124, THE MARK).
/// The one reading of the file's own records that [`Projector::owed_because`] answers and
/// [`Projector::rebuild`] pays on.
fn owed_by_file(conn: &Connection) -> Result<Vec<&'static str>, Error> {
    let mut causes = Vec::new();
    if rebuild_owed(conn)? {
        causes.push(OWED_OLDER_RULE);
    }
    Ok(causes)
}

/// What one [`Projector::prune`] reclaimed, both in the same transaction: the dead-run
/// decision/finding nodes it dropped (spec 21) and the superseded structural edges it reclaimed
/// beyond the retention boundary (spec 41). A plain count pair so `rigger reset --runs` reports
/// both, and a test can assert the superseded-edge reclamation off the return value without
/// reaching into the private edge table.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PruneStats {
    /// Dead-run / pre-boundary decision and finding nodes removed (spec 21).
    pub nodes: usize,
    /// Superseded structural edges (`valid_to IS NOT NULL`) reclaimed because they were retired
    /// before the retention boundary - a prior run's cruft the log can re-derive (spec 41). A LIVE
    /// edge (`valid_to IS NULL`) is never counted here, because it is never reclaimed.
    pub superseded_edges: usize,
}

/// What a [`Projector::rebuild`] that ran did (spec 101): what its run-closure prune removed from
/// the rebuilt graph - the counts `rigger reset --runs` reports for its own prune. What it passed
/// over is the file's own record, read by [`Projector::passed_over`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rebuilt {
    /// What the run-closure prune removed, however many passes it took to get there.
    pub pruned: PruneStats,
}

/// The class of the cause a fold pass records for an event whose own payload the judge
/// ([`super::check_fold_payload`]) rejects (spec 124, PASSED OVER WITH ITS CAUSE).
pub const PAYLOAD_REJECTED: &str = "payload rejected";

/// The cause a fold pass records under `class` for the payload `data` of a `type_` event when the
/// judge ([`super::check_fold_payload`]) rejects it - the class, then the judge's text, the one
/// spelling [`Projector::passed_over`] reads the class back from, up to its first `:` - or none
/// when the judge accepts it.
pub fn rejection(class: &str, type_: &str, data: &[u8]) -> Option<String> {
    let why = super::check_fold_payload(type_, data).err()?;
    Some([class, &why].join(": "))
}

/// How many positions a graph file records passed over ([`Projector::passed_over`]), by the class
/// of their cause (spec 124, CATCH-UP LINES).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PassedOver {
    /// Events whose own payload the judge rejects (`payload rejected`).
    pub rejected: usize,
}

impl PassedOver {
    /// Every position counted passed over.
    pub fn total(&self) -> usize {
        self.rejected
    }
}

/// What a seeding of a graph file's `caught_up` cursor answered ([`Projector::seed_cursor`], spec
/// 124, THE CURSOR AND ITS SEEDING).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seeding {
    /// Its transaction met another writer's write lock past the busy timeout: it read nothing,
    /// wrote nothing and issued no verdict, leaving the file to the next `rigger setup`'s seeding.
    Skipped,
    /// It ran under the write lock. `lost` is its prefix-violation verdict, a verdict of this
    /// seeding alone and no record of the file: the file misses an event of the log, or records
    /// one past the log's end, that only a rebuild repairs in order, so `rigger setup` names the
    /// lost fold ([`OWED_LOST_FOLD`]) as a cause after the ones the file records
    /// ([`Projector::owed_because`]), the two composed by [`causes_once_seeded`], and pays it with
    /// a rebuild.
    Ran { lost: bool },
}

/// The causes `rigger setup` names for the rebuild of a graph file that records the causes
/// `recorded` ([`Projector::owed_because`]) and whose seeding answered `seeding`: those, then the
/// lost fold ([`OWED_LOST_FOLD`]) when the seeding ran and found the prefix it folded violated
/// (spec 124, THE MARK). Setup owes the file its rebuild whenever it names a cause.
pub fn causes_once_seeded(mut recorded: Vec<&'static str>, seeding: &Seeding) -> Vec<&'static str> {
    if *seeding == (Seeding::Ran { lost: true }) {
        recorded.push(OWED_LOST_FOLD);
    }
    recorded
}

/// What one [`Projector::catch_up`] call answered (spec 124, ONE PROCEDURE).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatchUp {
    /// The file folds no catch-up of its own, for the cause named, and the call wrote nothing; a
    /// cause standing when the call starts answers before the log is opened.
    Inert(Inert),
    /// The call's page read past the cursor met no event - the file is caught up with the log - or
    /// the projector was handed no log to read one from ([`Projector::with_log`]).
    AtHead,
    /// The call folded one cut of the log past the cursor and advanced the cursor to its end;
    /// `recorded` counts the positions the cut's transaction itself wrote to the `applied` ledger.
    Advanced { recorded: usize },
    /// The call's cut met another writer - its write lock past the busy timeout, or, on both of its
    /// page reads, a cursor at or past the cut's last position or below where the page read from -
    /// and wrote nothing; `at` is the position its last page read from.
    Skipped { at: Position },
    /// The call failed - the log's open, its page read or the cut's fold - and wrote nothing; `at`
    /// is the position the call reads, or would read, its last page from, `error` the failure's
    /// text.
    Failed { at: Position, error: String },
}

/// Why a graph file folds no catch-up of its own ([`CatchUp::Inert`]): each is a file whose rebuild
/// seeds or repairs its cursor, or one no seeding has given a cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inert {
    /// The file a rebuild swapped in still owes that rebuild's tail ([`REBUILD_STATE`]).
    RebuildState,
    /// The file records an older fold rule ([`OWED_OLDER_RULE`]).
    OlderRule,
    /// The file's `caught_up` cursor was recorded beside another log than the one its projector
    /// folds, or beside none ([`log_switched`]; spec 124, A LOG SWITCHED UNDER A STANDING GRAPH):
    /// the prefix it claims is not of this log, so the rebuild setup's seeding owes the file
    /// ([`Seeding::Ran`]) seeds it anew.
    LogSwitched,
    /// The file carries no `caught_up` cursor.
    NoCursor,
}

/// What the one transaction frame ([`in_transaction`]) answers when it does not answer its
/// body's value (spec 124, EXISTING MECHANISMS): CONTENTION is read at the frame's IMMEDIATE open,
/// before [`be`] stringifies anything, so a caller decides by the variant, never by an error's
/// text.
#[derive(Debug)]
enum FrameError {
    /// The open met another connection's write lock past the busy timeout: SQLite's busy or
    /// locked code (CONTENTION), the error as SQLite gave it.
    Contended(rusqlite::Error),
    /// Every other failure: another open error, the body's, the settlement's or the commit's.
    Failed(Error),
    /// A direct fold met its own position already passed over: that position's `passed_over`
    /// cause, a recorded outcome rather than a lost fold, so the appender commits what the fold
    /// wrote beside it ([`Projector::guarded`]).
    Recorded(String),
    /// A catch-up cut ([`CatchUpCut`]) found the `caught_up` cursor, read again under its write
    /// lock, at the position named outside the call's read - at or past the cut's last position,
    /// another writer having folded it, or below the position the page read from, a swap having
    /// lowered it - and folded nothing: the call reads one page again from there.
    Reread(Position),
    /// A catch-up cut found, under its write lock, that the file folds no catch-up of its own, for
    /// the cause named, and folded nothing.
    Inert(Inert),
}

impl From<Error> for FrameError {
    fn from(e: Error) -> Self {
        FrameError::Failed(e)
    }
}

impl FrameError {
    /// The domain's error for this frame error, for a caller that is not a fold, and for a rebuild
    /// pass, which fails on CONTENTION as on any failure: the text the open's error gives as
    /// [`be`] gives it, the failure's own error, or the recorded cause. A catch-up cut's decline
    /// ([`FrameError::Reread`], [`FrameError::Inert`]) is never unframed: only a catch-up cut
    /// declines, and its call answers each decline itself ([`Projector::catch_up`]).
    fn unframed(self) -> Error {
        match self {
            FrameError::Contended(e) => be(e),
            FrameError::Failed(e) => e,
            FrameError::Recorded(cause) => Error(cause),
            FrameError::Reread(_) | FrameError::Inert(_) => {
                unreachable!("a catch-up cut's decline is answered by its call, never unframed")
            }
        }
    }
}

impl Projector {
    /// Open (or create) the graph at `path`, scoped to `project` - the plain project string
    /// that namespaces this project's streams (`project_identity` / `StoreLocation::identity`).
    /// Every fold stamps that scope on each node and edge. A pre-spec-28 graph.db (no `project`
    /// column) that owes no rebuild is migrated in place, backfilling its existing rows with this
    /// identity, so a single-project deployment behaves exactly as before; one that owes it is
    /// refolded under this identity by that rebuild.
    pub fn open(path: &str, project: &str) -> Result<Self, Error> {
        let conn = open_connection(path).map_err(be)?;
        // A file that owes its rebuild is opened exactly as it stands: no schema, no migration,
        // no ledger drop. Every write it needs happens inside the rebuild's one transaction, so an
        // open racing that rebuild - a read, or a folding command about to refuse - can neither
        // undo nor observe half of it, and never waits on its lock.
        if !rebuild_owed(&conn)? {
            open_current(&conn, project)?;
        }
        Ok(Projector {
            conn: Mutex::new(conn),
            project: project.to_string(),
            path: path.to_string(),
            owed_mark: owed_mark(path),
            log: None,
            store: String::new(),
            lock_ceiling: FOLD_LOCK_CEILING,
            holds_rebuild_lock: false,
        })
    }

    /// Take the rebuild lock of the graph file at `path` (spec 101), never waiting: the OS advisory
    /// lock on the zero-byte `<path>.lock` beside it, created if absent and never removed. Refused at
    /// once with the one text ([`REBUILD_IN_PROGRESS`]) while another holds it - a rebuild, whatever
    /// it is doing and whether or not it has written yet, or a `rigger reset` - touching nothing but
    /// the lock file the holder made; any other failure names the lock file.
    pub fn lock_rebuild(path: &str) -> Result<RebuildLock, Error> {
        take_rebuild_lock(path)?.ok_or_else(|| Error(REBUILD_IN_PROGRESS.to_string()))
    }

    /// Pay the rebuild this projector's graph file - the file it was opened on - owes (spec 101),
    /// and report whether there was one to pay - `None` when there was not, else what it did
    /// ([`Rebuilt`]): what its run-closure prune removed - owed because its caller found it so
    /// (`owed`: a cause the file records ([`Projector::owed_because`]), or the lost fold the
    /// seeding of the file's `caught_up` cursor found ([`Seeding::Ran`])), or because the file
    /// itself records an older fold rule, read again here. A rebuild's own unfinished work is paid
    /// too, with no owed cause: the shadow a stopped rebuild left standing, whose swap it finishes
    /// before any tail, and the tail past the cursor a swap put in place. What the pay does with
    /// its shadow is one classification ([`shadow_pay`], which [`Projector::rebuild_starts_fresh`]
    /// answers): a standing shadow is resumed when it is resumable ([`shadow_resumable`]) and
    /// replaced by a fresh one otherwise, and with none standing a cause owed starts a fresh one, a
    /// swapped-in tail standing or not, while no cause starts none. The [`owed_mark`] an older
    /// binary's lost fold left, no cause, is dropped once the rebuilt file is in place. The live
    /// file is this projector's own: its connection, held under its mutex for the whole rebuild, is
    /// the one the swap copies into and the tail folds through, under this projector's project.
    ///
    /// The rebuild folds `source` - the log's live selection, whose cost is bounded by the live
    /// projection rather than the log's age. It folds into a fresh SHADOW file beside the graph file, in the batches `source` hands it,
    /// each folded in the bounded transactions `fold_source` cuts it into, each transaction
    /// committed with the last position it folded and the run attribution it gathered
    /// ([`REBUILD_STATE`]), and each batch reported to `progress`; the live file is only ever read
    /// meanwhile. The transaction that opens the shadow's rebuild state also creates its
    /// `caught_up` cursor at the shadow's rebuild cursor ([`create_cursor`], spec 124): zero in a
    /// fresh shadow, and where a resumed one stands when a binary from before the cursor left it
    /// without one. Once `source` is exhausted, a private copy of the shadow, pruned as `rigger
    /// reset --runs` prunes the live graph and stamped with the current [`PROJECTION_VERSION`]
    /// ([`prune_a_copy`]), is put in place of the live file's content in ONE step - one write
    /// transaction that copies it page for page, so a racing open sees the old file, which still
    /// owes the rebuild, or the rebuilt one, never half of either - and then removed; what that
    /// prune removed is reported, read from the rebuild state, where it was stamped on the copy
    /// with the prune. The copy is SQLite's online backup rather than a rename of the file: a
    /// process holding the live file open (an agent's MCP session, the dashboard) keeps a valid
    /// connection and reads the rebuilt file from its next query, where a file renamed over one it
    /// holds would leave it reading the old file through a write-ahead log the new one shares. The
    /// shadow itself stays unpruned, and stamped with the fold rule it is folded under
    /// ([`open_shadow`]), until the swap ends by removing it ([`remove_shadow`]).
    ///
    /// The shadow is never pruned, because the drop set is not monotone in the log: an id a closed
    /// run recorded moves from drop to keep when the active run records it again, and a prune
    /// cannot be undone. So the shadow stays a pure fold of the live selection. The rebuild runs
    /// only under `_held`, the graph file's rebuild lock ([`Projector::lock_rebuild`]), whose path
    /// it never reads - the shadow and the pruned copy sit beside the file this projector was
    /// opened on - and which its caller took before this opened or created anything and holds
    /// through the fold, the swap and the tail until the cursor is dropped: a second rebuild is refused at its lock, never
    /// interleaved, whatever phase this one is in, and only this one ever has the shadow open
    /// ([`open_shadow`]), so it ends it by closing and then removing it ([`remove_shadow`]). An
    /// interrupted rebuild leaves the live file untouched, or swapped whole, and no lock - the OS
    /// releases it with the process - and the next one resumes from the shadow's last committed
    /// transaction without refolding it and prunes a fresh copy from the whole run attribution the
    /// shadow gathered - the same graph and the same counts a cold rebuild reaches, however far the
    /// pass before it got. A shadow that is not resumable - folded under another fold rule, so it
    /// holds what that rule gave, or without the run attribution it gathered - is discarded and the
    /// log folded from its start ([`shadow_resumable`], [`open_shadow`]).
    ///
    /// An emit that found the old file owing appended without folding, and did so before the
    /// copy: so after it `source` is read once more from the cursor alone, into the live file and
    /// reported to `progress`, and only then is the cursor dropped - the per-position guard makes
    /// that tail meet an emit that folded its own event exactly once. A rebuild interrupted in that
    /// tail finishes exactly the tail on the next call, with no owed cause - the swapped-in file
    /// carries the `caught_up` cursor its shadow opened with, so its seeding reads nothing, and the
    /// seeding of one swapped in without that cursor reads no position past the swapped-in
    /// rebuild cursor, the tail's own debt rather than a lost fold ([`Projector::seed_cursor`]) -
    /// and reports the counts stamped with the swap. Only then is the rebuild state dropped, the
    /// cursor and the attribution together, in the one transaction that sets the `caught_up`
    /// cursor at the last position the rebuild folded, creating it on a file swapped in without
    /// one.
    pub fn rebuild(
        &self,
        _held: &RebuildLock,
        owed: bool,
        source: &mut RebuildSource,
        progress: &mut dyn FnMut(RebuildProgress),
    ) -> Result<Option<Rebuilt>, Error> {
        let (path, project, store) = (
            self.path.as_str(),
            self.project.as_str(),
            self.store.as_str(),
        );
        let mut live = self.conn.lock().unwrap();
        let live = &mut *live;
        let shadow_path = shadow_of(path);
        match shadow_pay(path, live, owed, store)? {
            ShadowPay::Skip => {
                if !rebuild_tail_owed(live)? {
                    return Ok(None);
                }
            }
            pay => {
                let mut shadow = open_shadow(&shadow_path, pay.starts_fresh())?;
                schema(&shadow, project)?;
                in_transaction(&mut shadow, |tx| {
                    tx.execute_batch(REBUILD_STATE).map_err(be)?;
                    create_cursor(tx, rebuild_cursor(tx)?, store)
                })
                .map_err(FrameError::unframed)?;
                fold_source(&mut shadow, project, source, progress, true)?;
                if fold_rule(live)? == RECORDS_ONLY_RULE - 1 {
                    refuse_a_lossy_refold(&shadow, path, project)?;
                }
                // The graph the live one would hold, never a larger one: the prune `rigger reset
                // --runs` applies, derived from what the fold gathered, on a copy of the shadow.
                let copy = pruned_copy(path);
                let pruned = prune_a_copy(&shadow, &copy, project)?;
                // Every page in one step: one write transaction on the live file.
                rusqlite::backup::Backup::new(&pruned, live)
                    .map_err(be)?
                    .run_to_completion(i32::MAX, std::time::Duration::from_millis(50), None)
                    .map_err(be)?;
                drop(pruned);
                remove_if_present(&copy)?;
                // Paid: the swapped-in file holds every event the source held, so the mark an older
                // binary's lost fold left is dropped. Dropped before the tail, so one an older
                // binary writes while the tail runs stands, for setup to remove.
                if let Some(mark) = &self.owed_mark {
                    remove_if_present(mark)?;
                }
                // The swap ends with its shadow: until it is gone a rerun resumes it, and from then
                // on only the tail is owed.
                remove_shadow(shadow, &shadow_path)?;
            }
        }
        fold_source(live, project, source, progress, false)?;
        let pruned = live
            .query_row(
                "SELECT pruned_nodes, reclaimed_edges FROM rebuild_cursor",
                [],
                |r| {
                    Ok(PruneStats {
                        nodes: r.get::<_, i64>(0)? as usize,
                        superseded_edges: r.get::<_, i64>(1)? as usize,
                    })
                },
            )
            .map_err(be)?;
        in_transaction(live, |tx| {
            let through = rebuild_cursor(tx)?;
            create_cursor(tx, through, store)?;
            set_cursor(tx, CAUGHT_UP, through)?;
            tx.execute_batch(DROP_REBUILD_STATE).map_err(be)
        })
        .map_err(FrameError::unframed)?;
        Ok(Some(Rebuilt { pruned }))
    }

    /// Whether a rebuild of the graph file `held` names was left unfinished (spec 107): the file
    /// stands and has beside it a shadow a stopped rebuild left ([`shadow_stands`]), which the next
    /// [`Projector::rebuild`] resumes or, when it is not resumable, starts afresh, or holds the
    /// cursor whose tail it finishes. False when no graph file stands, whatever stands beside where
    /// it would be, and answered then, as over a standing shadow, without opening anything - so
    /// asking never makes a graph file. It takes the held rebuild lock, so no caller asks while a
    /// rebuild could be moving either sign. It writes no row, table or projection; a standing graph
    /// file with no shadow beside it is opened through the one opener [`Projector::rebuild`] opens
    /// it through, which checkpoints a write-ahead log a dead process left, as any open does.
    pub fn rebuild_unfinished(held: &RebuildLock) -> Result<bool, Error> {
        let path = held.path();
        if !Path::new(path).exists() {
            return Ok(false);
        }
        if shadow_stands(path) {
            return Ok(true);
        }
        rebuild_tail_owed(&open_connection(path).map_err(be)?)
    }

    /// Whether paying this file's rebuild ([`Projector::rebuild`], handed the same `owed`) starts
    /// a fresh shadow and folds the log from its start (spec 124): when no shadow stands, a cause
    /// is owed - `owed`, or one the file itself records ([`owed_by_file`]) - whether or not a
    /// swapped-in tail stands; when one stands, it is not resumable ([`shadow_resumable`]). False
    /// when the pay resumes a standing shadow, cause or not, finishes a tail no cause is owed
    /// beside, or owes nothing. The one answer the pay picks its shadow by ([`shadow_pay`]). It
    /// takes the held rebuild lock, so between this answer and the pay nothing moves the shadow,
    /// its fold rule or the tail, and no cause is added: the [`owed_mark`] an older binary's lost
    /// fold writes is none. It makes no shadow and folds nothing: a standing one is opened
    /// without being created and only read, which rolls back a journal a killed rebuild left hot,
    /// as any read of it does.
    pub fn rebuild_starts_fresh(&self, _held: &RebuildLock, owed: bool) -> Result<bool, Error> {
        let live = self.conn.lock().unwrap();
        Ok(shadow_pay(&self.path, &live, owed, &self.store)?.starts_fresh())
    }

    /// Why this file says it owes its rebuild (spec 101): the older fold rule it records
    /// ([`OWED_OLDER_RULE`]), and none when it records none - the [`owed_mark`] beside it is no
    /// cause (spec 124, THE MARK). Read from the file on every call, so another process's
    /// [`Projector::rebuild`] is seen at once; it reads nothing else and writes nothing.
    pub fn owed_because(&self) -> Result<Vec<&'static str>, Error> {
        owed_by_file(&self.conn.lock().unwrap())
    }

    /// How many positions this file records passed over, by the class of their cause (spec 124,
    /// CATCH-UP LINES): its `passed_over` rows as they stand, whoever wrote them, each counted by
    /// the class its cause begins with ([`rejection`]) - [`PAYLOAD_REJECTED`], the only class a
    /// fold pass writes ([`decide_step`]). None on
    /// a file without the table, such as one that owes its rebuild, which is opened as it stands.
    /// Reads, and writes nothing.
    pub fn passed_over(&self) -> Result<PassedOver, Error> {
        let conn = self.conn.lock().unwrap();
        let mut counts = PassedOver::default();
        if !has_table(&conn, "passed_over")? {
            return Ok(counts);
        }
        let mut classes = conn
            .prepare(
                "SELECT substr(cause, 1, instr(cause || ':', ':') - 1), count(*)
                 FROM passed_over GROUP BY 1",
            )
            .map_err(be)?;
        let classes = classes
            .query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as usize))
            })
            .map_err(be)?;
        for class in classes {
            let (class, n) = class.map_err(be)?;
            if class == PAYLOAD_REJECTED {
                counts.rejected += n;
            }
        }
        Ok(counts)
    }

    /// Seed this file's `caught_up` cursor against the log (spec 124, THE CURSOR AND ITS
    /// SEEDING), in ONE immediate transaction ([`Projector::transact`]) that takes the write lock
    /// before it reads anything, and answer its verdict ([`Seeding`]). A transaction that meets
    /// another writer's lock past the busy timeout is [`Seeding::Skipped`], having read and written
    /// nothing; any other failure is the error. A file already carrying `caught_up` keeps its
    /// cursor and its ledger is read no further: nothing at or below the cursor is missing from
    /// it. Its verdict is lost when the cursor was recorded beside another log than the one this
    /// projector folds ([`Projector::with_store_identity`]), or beside none - a cursor whose table
    /// predates that identity - as when the event store is switched under the file (spec 124, A
    /// LOG SWITCHED UNDER A STANDING GRAPH), and otherwise only against `log_head`, the log's head
    /// its caller read once ([`EventStore::log_head`]): a head below the cursor, or no head beside
    /// a cursor above 0 - a log shorter than the prefix the cursor claims, as when the log is
    /// replaced under the file; beside a head at or above the cursor, or no head beside a cursor
    /// at 0, it loses nothing. Neither reads a log row.
    ///
    /// On a file without it the ledger is read against the positions `live` streams - the log's
    /// live selection, whose last position is the log's head - at or below the highest position
    /// the ledger records and, on a file a rebuild swapped in whose tail is still owed, at or below
    /// that rebuild's cursor, the tail's own debt lying past it. Each missing position's store
    /// stamp is read through `stamp_of` once the stream is read: the missing position is old when
    /// the stamp is more than [`CATCH_UP_HORIZON`] before `now`, or when `stamp_of` answers none,
    /// and young otherwise, a stamp after `now` included. A file that owes its rebuild for an older
    /// fold rule, or still owes a rebuild's tail, gets no cursor - the rebuild seeds it - and an
    /// old missing position is its verdict. Any other file is seeded at the highest position its
    /// ledger records, or just below the lowest young missing position, or at 0 for an empty
    /// ledger beside an empty log; an old missing position, an empty ledger beside a non-empty
    /// log, or a non-empty one beside an empty log or above its head violates the prefix, which
    /// only a rebuild repairs in order, so no cursor is created and the verdict is lost.
    pub fn seed_cursor(
        &self,
        live: &mut PositionSource,
        stamp_of: &mut StampLookup,
        log_head: Option<Position>,
        now: SystemTime,
    ) -> Result<Seeding, Error> {
        let seeded = self.transact(|tx| -> Result<Seeding, Error> {
            if let Some((cursor, recorded)) = caught_up_beside(tx)? {
                return Ok(Seeding::Ran {
                    lost: log_switched(recorded.as_deref(), &self.store)
                        || log_head.map_or(cursor > 0, |head| head < cursor),
                });
            }
            let highest = highest_applied(tx)?;
            let tail = rebuild_tail_from(tx)?;
            let through = match tail {
                Some(cursor) => highest.map(|h| h.min(cursor)),
                None => highest,
            };
            let mut head = None;
            let mut missing = Vec::new();
            live(&mut |positions| {
                for &position in positions
                    .iter()
                    .filter(|&&p| through.is_some_and(|t| p <= t))
                {
                    if !has_applied(tx, position)? {
                        missing.push(position);
                    }
                }
                head = positions.last().copied().or(head);
                Ok(())
            })?;
            let mut old_miss = false;
            for &position in &missing {
                if older_than(stamp_of(position)?, CATCH_UP_HORIZON, now) {
                    old_miss = true;
                    break;
                }
            }
            if rebuild_owed(tx)? || tail.is_some() {
                return Ok(Seeding::Ran { lost: old_miss });
            }
            let cursor = match (highest, head) {
                (None, None) => Some(0),
                (Some(h), Some(head)) if h <= head && !old_miss => {
                    Some(missing.first().map_or(h, |&young| young - 1))
                }
                _ => None,
            };
            if let Some(at) = cursor {
                create_cursor(tx, at, &self.store)?;
            }
            Ok(Seeding::Ran {
                lost: cursor.is_none(),
            })
        });
        match seeded {
            Err(FrameError::Contended(_)) => Ok(Seeding::Skipped),
            seeded => seeded.map_err(FrameError::unframed),
        }
    }

    /// This projector, folding forward from the event log `open` opens ([`Projector::catch_up`],
    /// spec 124, WIRING): the store holding the log this file folds, already scoped to this
    /// projector's project, which each call reads as it is handed, laying no scope of its own over
    /// it. `open` is called by the first call that reads the log, never
    /// here, and the log it opens is kept for the projector's life; an open that fails keeps
    /// nothing, so the next call opens again.
    pub fn with_log(
        mut self,
        open: impl Fn() -> Result<Arc<dyn EventStore>, Error> + Send + Sync + 'static,
    ) -> Self {
        self.log = Some(CatchUpLog {
            open: Box::new(open),
            opened: Mutex::new(None),
        });
        self
    }

    /// This projector, folding the event log whose identity is `store` (spec 124, A LOG SWITCHED
    /// UNDER A STANDING GRAPH): the text its seeding ([`Projector::seed_cursor`]) and its rebuild's
    /// shadow open and drop ([`Projector::rebuild`]) record beside the `caught_up` cursor they
    /// create ([`create_cursor`]), and that the seeding and every [`Projector::catch_up`] call
    /// compare the cursor's with ([`log_switched`]).
    pub fn with_store_identity(mut self, store: String) -> Self {
        self.store = store;
        self
    }

    /// This projector, its appender waiting at most `ceiling` for its own positions to fold
    /// ([`Projector::guarded`], spec 124, THE APPENDER) in place of [`FOLD_LOCK_CEILING`]: what a
    /// test injects, compiled for this crate's tests alone, since production never calls it.
    #[cfg(test)]
    pub(crate) fn with_lock_ceiling(mut self, ceiling: Duration) -> Self {
        self.lock_ceiling = ceiling;
        self
    }

    /// This projector, knowing its process holds `held`, the rebuild lock [`Projector::lock_rebuild`]
    /// answered, when that lock is of this projector's own file (spec 124, THE APPENDER) - the two
    /// paths naming one file once each is made absolute against the working directory, so a
    /// relative and an absolute spelling of it match; none, another file's, or a path that cannot
    /// be made absolute records nothing. An appender of a projector told so whose call meets the
    /// file's rebuild state answers the unfinished-rebuild text at once
    /// ([`Projector::fold_by_calls`]), since only that process's tail folds its event.
    pub fn with_rebuild_lock(mut self, held: Option<&RebuildLock>) -> Self {
        let absolute = |path: &str| std::path::absolute(path).ok();
        self.holds_rebuild_lock = held.is_some_and(|lock| {
            absolute(&lock.path).is_some_and(|locked| absolute(&self.path) == Some(locked))
        });
        self
    }

    /// FOLD FORWARD FROM THE CURSOR (spec 124, ONE PROCEDURE): fold this file forward from its
    /// `caught_up` cursor by one cut of the log ([`Projector::with_log`]), in ONE immediate
    /// transaction, and answer what the call did ([`CatchUp`]); a caller that needs the file at the
    /// log's head loops calls until one answers [`CatchUp::AtHead`] or stops.
    ///
    /// The call first reads, with no transaction open, whether the file folds no catch-up of its
    /// own, each cause in order: it holds a rebuild's state, records an older fold rule over what
    /// it folded, carries no cursor, or carries one recorded beside another log than the one this
    /// projector folds, or beside none ([`Inert`]); the first standing is the answer, read without
    /// opening the log, and the [`owed_mark`] an older binary's lost fold left beside it is none. A
    /// projector handed no log answers [`CatchUp::AtHead`]. Otherwise the call reads one page of
    /// the log past the cursor - the first [`FOLD_BATCH`] events of the project's own log as it is
    /// handed, or every one there is when fewer are ([`EventStore::read_all_page`]) - with no lock
    /// of the file held: a page without an event is [`CatchUp::AtHead`]. The page's events of the
    /// project's run stream are its positions to decide; its first transaction cut
    /// ([`fold_transactions`]) is decided and folded by THE DECIDE STEP ([`decide_step`]), the step a
    /// rebuild's fold pass shares. The cut's transaction reads the cursor again under
    /// the write lock and ends by moving it to the cut's last position, or to the page's last
    /// position when the cut holds every run-stream event of the page - a page of other streams'
    /// events alone moves it there folding nothing - and the call answers [`CatchUp::Advanced`],
    /// counting the positions that transaction itself recorded in the `applied` ledger.
    ///
    /// Under the write lock the cut answers a rebuild's state, an older fold rule, a dropped
    /// cursor or one recorded beside another log or none that another writer brought about after
    /// the first read as [`CatchUp::Inert`],
    /// folding nothing. A cursor that another writer moved to or past the position the cut would
    /// move it to, or that a swap lowered below where the page was read from, rolls the cut back
    /// and the call reads one page again from that cursor; the second such page answers
    /// [`CatchUp::Skipped`] at the position it was read from. A cursor moved inside the cut is no
    /// outrun: the positions it passed are in the ledger, and the cut passes them. A cut that
    /// meets another writer's write lock past the busy timeout answers [`CatchUp::Skipped`] at the
    /// position its page was read from; a log that fails to open, a page read that fails, and a
    /// cut whose fold fails answer [`CatchUp::Failed`] at
    /// the position the call reads, or would read, its last page from, with the failure's text. Each
    /// of those writes nothing. The connection's lock is held for the cut's transaction and
    /// released for the page read. Only a failure to read the file's own standing is an error.
    pub fn catch_up(&self) -> Result<CatchUp, Error> {
        let mut from = match standing_cursor(&self.conn.lock().unwrap(), &self.store)? {
            Ok(at) => at,
            Err(inert) => return Ok(CatchUp::Inert(inert)),
        };
        let Some(log) = &self.log else {
            return Ok(CatchUp::AtHead);
        };
        let log = match log.opened() {
            Ok(log) => log,
            Err(e) => {
                return Ok(CatchUp::Failed {
                    at: from,
                    error: e.0,
                })
            }
        };
        let mut outrun = false;
        loop {
            let page = match log.read_all_page(from, &Filter::default(), FOLD_BATCH) {
                Ok(page) => page,
                Err(e) => {
                    return Ok(CatchUp::Failed {
                        at: from,
                        error: e.to_string(),
                    })
                }
            };
            let Some(last) = page.last().map(|e| e.position) else {
                return Ok(CatchUp::AtHead);
            };
            let run: Vec<Event> = page.into_iter().filter(on_the_run_stream).collect();
            let part = fold_transactions(&run).next().unwrap_or_default();
            let through = match part.last() {
                Some(cut_last) if part.len() < run.len() => cut_last.position,
                _ => last,
            };
            let mut cut = CatchUpCut {
                store: &self.store,
                from,
                lock_at: from,
                before: 0,
                recorded: 0,
            };
            let decided = decide_step(
                &mut &self.conn,
                &self.project,
                part,
                through,
                &mut cut,
                false,
            );
            match decided {
                Ok(()) => {
                    return Ok(CatchUp::Advanced {
                        recorded: cut.recorded,
                    })
                }
                Err(FrameError::Reread(at)) if !outrun => {
                    outrun = true;
                    from = at;
                }
                Err(FrameError::Reread(_) | FrameError::Contended(_)) => {
                    return Ok(CatchUp::Skipped { at: from })
                }
                Err(FrameError::Inert(inert)) => return Ok(CatchUp::Inert(inert)),
                Err(failed) => {
                    return Ok(CatchUp::Failed {
                        at: from,
                        error: failed.unframed().0,
                    })
                }
            }
        }
    }

    /// Drop the [`owed_mark`] of the graph file at `path` once that file is gone: the mark speaks
    /// of a fold an older binary lost from that file alone, never of a file made in its place. A
    /// mark whose file stands is kept.
    pub fn forget_orphaned_mark(path: &str) -> Result<(), Error> {
        match owed_mark(path).filter(|_| !Path::new(path).exists()) {
            Some(mark) => remove_if_present(mark),
            None => Ok(()),
        }
    }

    /// Drop the [`owed_mark`] an older binary's lost fold left beside this file once the file
    /// carries its `caught_up` cursor (spec 124, THE MARK): what the file lost is the seeding's to
    /// find ([`Seeding::Ran`]) and what lies past the cursor its catch-up's to fold, so the mark is
    /// no cause. A mark beside a file without the cursor is kept; an in-memory graph has none.
    pub fn forget_cursored_mark(&self) -> Result<(), Error> {
        match &self.owed_mark {
            Some(mark) if carries_caught_up(&self.conn.lock().unwrap())? => remove_if_present(mark),
            _ => Ok(()),
        }
    }

    /// Remove the stale pruned copy ([`pruned_copy`]) of the graph file at `path` that a rebuild's
    /// stopped swap left, and answer what it found ([`StaleCopy`]) - unless another holds the
    /// rebuild lock, a rebuild or a `rigger reset`, in which case the copy may be the one a live
    /// swap is using, so it is kept and that is answered. The lock ([`Projector::lock_rebuild`])
    /// is taken first, never waiting, and held while the copy is removed, so no copy a swap is
    /// using is ever removed under it; with no copy nothing is taken and nothing is made. The
    /// shadow is never read or removed here, since a rebuild resumes from it.
    pub fn forget_stale_copy(path: &str) -> Result<StaleCopy, Error> {
        let copy = pruned_copy(path);
        if !Path::new(&copy).exists() {
            return Ok(StaleCopy::Absent);
        }
        match take_rebuild_lock(path)? {
            Some(_held) => remove_if_present(&copy).map(|()| StaleCopy::Removed),
            None => Ok(StaleCopy::InUse),
        }
    }

    /// Run `body` in ONE transaction on this file's connection, held under its lock for the
    /// whole of it ([`in_transaction`]): the one lock -> transaction -> body -> commit frame,
    /// taken by each method that writes in a transaction. The lock is released when this returns.
    fn transact<T, E>(
        &self,
        body: impl FnOnce(&Transaction) -> Result<T, E>,
    ) -> Result<T, FrameError>
    where
        FrameError: From<E>,
    {
        in_transaction(&mut self.conn.lock().unwrap(), body)
    }

    /// THE APPENDER (spec 124): fold the appender's own `events` into this file, and answer what the
    /// direct fold's `fold` answered, or none when the calls folded them. It refuses before
    /// folding when the file owes its rebuild ([`REBUILD_OWED`]), and writes no mark for any
    /// failure: a position it leaves unfolded is missing from the `applied` ledger, which the next
    /// fold through the folding store or the next `rigger setup` reads.
    ///
    /// Each attempt first reads, with no transaction open, whether the calls fold the events
    /// ([`Projector::folds_by_calls`]): on a projector handed the log, for events all of the
    /// project's run stream, on a file carrying `caught_up`, it folds them by its calls
    /// ([`Projector::fold_by_calls`]), and a call meeting no cursor turns to the direct fold.
    /// Otherwise `fold` runs in ONE immediate transaction ([`in_transaction`]), which reads that
    /// again under the write lock and, finding the calls fold them - a seeding landed between the
    /// two reads - ends unwritten and turns to the calls. An open meeting another writer's write
    /// lock past the busy timeout ([`FrameError::Contended`]) is waited out: one busy interval of
    /// the connection with its lock released, then the attempt again, each open after the first
    /// made only while the lowest own position is no older than the ceiling ([`Projector::past_ceiling`]),
    /// else answered with the write-lock text ([`Projector::past_the_ceiling`]). A direct fold
    /// that answers its own position already recorded passed over ([`FrameError::Recorded`])
    /// commits what it wrote beside it and answers that cause alone; any other failure rolls the
    /// transaction back and answers its own text. Each text a direct fold
    /// answers but the recorded cause ends with [`DIRECT_PAYS`] when the events are of the run
    /// stream, and with nothing when they are not, since no later fold reads another stream.
    fn guarded<T>(
        &self,
        events: &[Event],
        mut fold: impl FnMut(&Transaction) -> Result<T, FrameError>,
    ) -> Result<Option<T>, Error> {
        if self.rebuild_owed()? {
            return Err(Error(REBUILD_OWED.to_string()));
        }
        let streamed = events.iter().all(on_the_run_stream);
        let pays = if streamed { DIRECT_PAYS } else { "" };
        let busy = self.busy_interval()?;
        let mut retry = false;
        loop {
            let by_calls = self.folds_by_calls(&self.conn.lock().unwrap(), streamed)?;
            if by_calls && self.fold_by_calls(events, busy)? {
                return Ok(None);
            }
            let mut conn = self.conn.lock().unwrap();
            if retry && self.past_ceiling(events) {
                return Err(self.past_the_ceiling(Short::Contended, pays));
            }
            let folded = in_transaction(&mut conn, |tx| {
                if self.folds_by_calls(tx, streamed)? {
                    return Ok(None);
                }
                match fold(tx) {
                    Ok(folded) => Ok(Some(Ok(folded))),
                    // A recorded cause commits what the fold wrote beside it.
                    Err(FrameError::Recorded(cause)) => Ok(Some(Err(cause))),
                    Err(failed) => Err(failed),
                }
            });
            drop(conn);
            match folded {
                Ok(Some(Ok(folded))) => return Ok(Some(folded)),
                Ok(Some(Err(cause))) => return Err(Error(cause)),
                Ok(None) => {}
                Err(FrameError::Contended(_)) => {
                    retry = true;
                    std::thread::sleep(busy);
                }
                Err(failed) => return Err(Error(format!("{}{pays}", failed.unframed().0))),
            }
        }
    }

    /// Whether an appender folds its events by its calls (spec 124, THE APPENDER): this projector
    /// was handed the log ([`Projector::with_log`]), the events are all of the project's run
    /// stream (`streamed`), and the file behind `conn` carries the `caught_up` cursor.
    fn folds_by_calls(&self, conn: &Connection, streamed: bool) -> Result<bool, Error> {
        Ok(self.log.is_some() && streamed && carries_caught_up(conn)?)
    }

    /// THE APPENDER's calls (spec 124): call [`Projector::catch_up`] until the appender's
    /// success test passes, and answer true then, or false when a call or the test meets no
    /// `caught_up` cursor, the direct fold folding the events instead.
    ///
    /// A call meeting a cursor recorded beside another log or none ([`Inert::LogSwitched`])
    /// answers the lost fold's refusal ([`REBUILD_OWED_FOR_LOST_FOLD`]) at once, before the
    /// success test, which would read that log's prefix as this one's. After every other call but
    /// one meeting no cursor, the success test reads the cursor and the first own position
    /// recorded passed over in one statement outside any transaction ([`own_standing`]): such a
    /// row answers its cause at once, with no suffix, since no later fold folds it; the last own
    /// position at or below the cursor is folded. Still short, the call's answer decides. An older
    /// fold rule answers [`REBUILD_OWED`] at once, the file's
    /// rebuild state the unfinished-rebuild text at once when this projector's process holds its
    /// rebuild lock ([`Projector::with_rebuild_lock`]), and a failed call its error at once;
    /// otherwise, past the ceiling ([`Projector::past_ceiling`]), it answers what the call met -
    /// the catch-up text after a call that advanced or met the log's head, the write-lock text
    /// after a skipped one, or the unfinished-rebuild text - each but `REBUILD_OWED` ending with
    /// [`CALL_PAYS`]. Inside the ceiling a call that
    /// advanced is followed by the next at once, and any other by one busy interval `busy` with the
    /// connection's lock released.
    fn fold_by_calls(&self, events: &[Event], busy: Duration) -> Result<bool, Error> {
        let positions: Vec<Position> = events.iter().map(|e| e.position).collect();
        let last = positions.iter().copied().max();
        let positions = serde_json::to_string(&positions).map_err(be)?;
        loop {
            let call = self.catch_up()?;
            match call {
                CatchUp::Inert(Inert::NoCursor) => return Ok(false),
                CatchUp::Inert(Inert::LogSwitched) => {
                    return Err(Error(REBUILD_OWED_FOR_LOST_FOLD.to_string()))
                }
                _ => {}
            }
            let standing = own_standing(&self.conn.lock().unwrap(), &positions)?;
            match standing {
                None => return Ok(false),
                Some((_, Some(cause))) => return Err(Error(cause)),
                Some((cursor, None)) if last.is_none_or(|last| last <= cursor) => return Ok(true),
                Some(_) => {}
            }
            let (short, waits) = match call {
                CatchUp::Advanced { .. } => {
                    (self.past_the_ceiling(Short::CatchingUp, CALL_PAYS), false)
                }
                CatchUp::AtHead => (self.past_the_ceiling(Short::CatchingUp, CALL_PAYS), true),
                CatchUp::Skipped { .. } => {
                    (self.past_the_ceiling(Short::Contended, CALL_PAYS), true)
                }
                CatchUp::Inert(Inert::RebuildState) => {
                    let unfinished = Error(format!("{UNFINISHED_REBUILD}{CALL_PAYS}"));
                    if self.holds_rebuild_lock {
                        return Err(unfinished);
                    }
                    (unfinished, true)
                }
                // An older fold rule; a call meeting no cursor turned direct above, and one
                // meeting a cursor of another log answered there.
                CatchUp::Inert(_) => return Err(Error(REBUILD_OWED.to_string())),
                CatchUp::Failed { error, .. } => return Err(Error(format!("{error}{CALL_PAYS}"))),
            };
            if self.past_ceiling(events) {
                return Err(short);
            }
            if waits {
                std::thread::sleep(busy);
            }
        }
    }

    /// Whether an appender of `events` has waited past its ceiling (spec 124, THE APPENDER): the
    /// stamp of its lowest own position is older now than [`Projector::with_lock_ceiling`]'s
    /// ceiling, [`FOLD_LOCK_CEILING`] unless a test injects another ([`older_than`]) - or it has no
    /// position to wait for.
    fn past_ceiling(&self, events: &[Event]) -> bool {
        let lowest = events.iter().min_by_key(|e| e.position);
        older_than(
            lowest.map(|e| e.recorded_at),
            self.lock_ceiling,
            SystemTime::now(),
        )
    }

    /// The not-folded error of an appender past its ceiling (spec 124, CAUSE TEXTS), what held it
    /// `short` spelled with the ceiling it held ([`held_for`]) and ending with its path's suffix
    /// `pays`.
    fn past_the_ceiling(&self, short: Short, pays: &str) -> Error {
        let held = held_for(self.lock_ceiling);
        Error(match short {
            Short::Contended => format!("graph.db's write lock stayed held past {held}{pays}"),
            Short::CatchingUp => {
                format!("graph.db's catch-up had not reached this event after {held}{pays}")
            }
        })
    }

    /// The interval an appender waits between two attempts (spec 124, THE APPENDER): one busy
    /// timeout of this file's connection, as the connection reads it.
    fn busy_interval(&self) -> Result<Duration, Error> {
        self.conn
            .lock()
            .unwrap()
            .query_row("PRAGMA busy_timeout", [], |r| r.get::<_, u64>(0))
            .map(Duration::from_millis)
            .map_err(be)
    }

    /// The WHOLE live projection for this project: every node plus every currently-valid edge
    /// (`valid_to IS NULL`), read DIRECTLY (spec 45, criterion 2 - the direct-projection read the
    /// dedicated `/api/graph` provider consults). Unlike [`ContextGraph::subgraph`] it takes no seed
    /// and walks no reachability CTE - it returns the full graph, so a whole-graph overview and a
    /// seeded neighborhood both reach any node the projection holds (the fix for the never-built
    /// repo whose run-seed set is empty). Read isolation (spec 28, criterion 2): scoped to
    /// `self.project` exactly like `subgraph`, so on a shared backend it returns only this project's
    /// nodes and edges. Deterministic by construction (spec 45): the rows are ORDERed so the read
    /// itself sorts, independent of any downstream fold. Read-only; a fresh/empty graph yields an
    /// empty [`Graph`], never an error.
    pub fn whole(&self) -> Result<Graph, Error> {
        let conn = self.conn.lock().unwrap();
        let mut nstmt = conn
            .prepare(
                "SELECT id, kind, attrs FROM nodes
                 WHERE project = ?1
                 ORDER BY id",
            )
            .map_err(be)?;
        let nodes: Vec<Node> = nstmt
            .query_map(params![self.project], row_to_node)
            .map_err(be)?
            .collect::<Result<_, _>>()
            .map_err(be)?;

        let mut estmt = conn
            .prepare(
                "SELECT from_id, to_id, rel, valid_from, source, tier FROM edges
                 WHERE valid_to IS NULL AND project = ?1
                 ORDER BY from_id, to_id, rel, valid_from",
            )
            .map_err(be)?;
        let edges: Vec<Edge> = estmt
            .query_map(params![self.project], row_to_edge)
            .map_err(be)?
            .collect::<Result<_, _>>()
            .map_err(be)?;

        Ok(Graph { nodes, edges })
    }

    /// Prune the given nodes and every edge that touches them from the graph, returning the
    /// number of nodes actually removed (spec 21, unit 2). This is the single graph-mutation
    /// authority `rigger reset --runs` uses to shed dead-run noise: the composition root
    /// derives the superseded / pre-boundary decision and finding node ids from the ONE
    /// run-attribution primitive (`run::run_attribution` + `run::current_run_id`) and passes
    /// them here. A `LessonLearned` node is never in that set (a lesson is exempt from
    /// attribution) and the active run's nodes are never in it, so this deletes EXACTLY what
    /// it is given - the keep invariant (every lesson plus the active run, including an id
    /// reused across a dead run AND the active run) is the caller's derivation, this is only
    /// the mutation.
    ///
    /// The `applied` position ledger is left UNTOUCHED, so a later replay of the same events
    /// is a no-op that never resurrects a pruned node: the prune drops from the graph WITHOUT
    /// wiping the store, which is exactly `reset --runs`'s contract. Both deletes run in one
    /// transaction so a pruned node never outlives its edges (or the reverse) on a crash.
    ///
    /// `superseded_before` extends the SAME prune authority to reclaim the superseded-edge
    /// accumulation (spec 41): when `Some(boundary)` it also reclaims every SUPERSEDED structural
    /// edge (`valid_to IS NOT NULL`) retired before `boundary` - a nanosecond-since-epoch cutoff in
    /// the graph's own [`to_nanos`] time base, derived by the composition root from the active run's
    /// `RunStarted` (the SAME run boundary that keeps the active run's nodes). Such an edge is a
    /// prior run's dead cruft: no live query reads it (grounding filters `valid_to IS NULL`) and the
    /// log re-derives it on a rebuild. A LIVE edge (`valid_to IS NULL`) is NEVER matched, so the safe
    /// superset grounding reads is untouched; an edge retired at or after the boundary (recent
    /// history) is kept, so the window is a bounded, not cumulative, tail. `None` reclaims no edge
    /// (a legacy store with no run boundary, or a node-only prune), preserving the pre-spec-41
    /// behavior. Project-scoped like the node deletes, and in the SAME transaction, so the whole
    /// prune is atomic.
    pub fn prune(
        &self,
        node_ids: &[String],
        superseded_before: Option<i64>,
    ) -> Result<PruneStats, Error> {
        self.transact(|tx| prune_in(tx, &self.project, node_ids, superseded_before))
            .map_err(FrameError::unframed)
    }

    /// A read-only PREVIEW of what [`prune`] would remove (spec 68, "the reset surface"): the
    /// SAME two predicates [`prune`]'s deletes select, each run as a `SELECT COUNT(*)` instead of
    /// a `DELETE` - how many of `node_ids` are actually present as this project's nodes, and how
    /// many edges are retired before `superseded_before`. No transaction, no row touched, no
    /// write lock taken, so `rigger reset`'s bare-menu preview can read this on every invocation
    /// without ever pruning anything itself, and its printed numbers can never drift from what a
    /// real `--runs` removes because both read the identical WHERE clauses.
    ///
    /// Scoped to `self.project` exactly like [`prune`] - a shared backend never counts another
    /// project's same-id node or edge.
    ///
    /// The `superseded_edges` count EXCLUDES any edge that touches a `node_ids` member. `prune`
    /// deletes every edge referencing a dropped node FIRST, unconditionally, via the same
    /// transaction's node-cascade delete - so such an edge is already gone by the time `prune`'s
    /// own boundary delete runs and is never counted toward ITS `superseded_edges`. A decision
    /// that superseded a prior GOVERNS-bearing decision (invalidating that predecessor's edges)
    /// and is itself later dropped as a dead-run node is the exact shape this closes: without the
    /// exclusion, this preview would count that predecessor's edges under BOTH `nodes` (via the
    /// node cascade) and `superseded_edges` (via the boundary predicate), overstating what a real
    /// prune actually removes - the one drift this function exists to make impossible.
    pub fn count_prunable(
        &self,
        node_ids: &[String],
        superseded_before: Option<i64>,
    ) -> Result<PruneStats, Error> {
        let ids_json = serde_json::to_string(node_ids).map_err(be)?;
        let conn = self.conn.lock().unwrap();
        let nodes: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM nodes
                 WHERE id IN (SELECT value FROM json_each(?1)) AND project = ?2",
                params![ids_json, self.project],
                |r| r.get(0),
            )
            .map_err(be)?;
        let superseded_edges: i64 = match superseded_before {
            Some(before) => conn
                .query_row(
                    &superseded_edges_sql(
                        "SELECT COUNT(*)",
                        "AND from_id NOT IN (SELECT value FROM json_each(?3))
                         AND to_id NOT IN (SELECT value FROM json_each(?3))",
                    ),
                    params![before, self.project, ids_json],
                    |r| r.get(0),
                )
                .map_err(be)?,
            None => 0,
        };
        Ok(PruneStats {
            nodes: nodes.max(0) as usize,
            superseded_edges: superseded_edges.max(0) as usize,
        })
    }

    /// Compact the on-disk graph file after a [`prune`], returning the bytes reclaimed (spec 46,
    /// criterion 3). [`prune`] drops superseded projection ROWS, but SQLite keeps the freed pages
    /// inside `graph.db` on a freelist for reuse, so the file stays as LARGE on disk as before the
    /// prune even though those rows are gone. `VACUUM` rebuilds the database without those free
    /// pages, then a `TRUNCATE` checkpoint folds the WAL side-file back so the MAIN file actually
    /// shrinks on disk immediately (not only at the next checkpoint), which is what a consumer
    /// inspecting the file then sees.
    ///
    /// This reclaims DISK ONLY. `VACUUM` changes NO query result and gives NO query or fold
    /// speedup: the row-count reduction that speeds reads is [`prune`]'s job, not `compact`'s. The
    /// projection is a persistent, incrementally-maintained file - `apply` folds only unseen log
    /// positions and `open` runs idempotent migrations without re-folding history - so a
    /// freelist-bloated file is never itself a slower query or a slower open; the only thing
    /// `compact` changes is the file's SIZE on disk.
    ///
    /// It is the DISK counterpart to `prune`'s ROW reclamation and the SAME graph-mutation
    /// authority - never a second connection opened elsewhere. It is a pure projection-file
    /// rebuild: it never touches the event log (the source of truth) and never changes a query
    /// result, only the file size, so it is safe to run unconditionally after every prune. The
    /// reclaimed byte count is the drop in `page_count * page_size` across the `VACUUM`; a
    /// no-op-shaped compaction (nothing was freed) simply reclaims 0 bytes.
    pub fn compact(&self) -> Result<u64, Error> {
        let conn = self.conn.lock().unwrap();
        let page_size: i64 = conn
            .query_row("PRAGMA page_size", [], |r| r.get(0))
            .map_err(be)?;
        // `page_count` counts the freelist pages the prune's deletes freed, so this is the
        // pre-VACUUM file size; after the VACUUM it is the compacted count.
        let before: i64 = conn
            .query_row("PRAGMA page_count", [], |r| r.get(0))
            .map_err(be)?;
        conn.execute_batch("VACUUM").map_err(be)?;
        // Fold the WAL back into the main file so the shrink lands on disk now, not at some later
        // checkpoint - the reported reclamation must match the file size on disk.
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
            .map_err(be)?;
        let after: i64 = conn
            .query_row("PRAGMA page_count", [], |r| r.get(0))
            .map_err(be)?;
        let reclaimed_pages = before.saturating_sub(after).max(0) as u64;
        Ok(reclaimed_pages * page_size.max(0) as u64)
    }

    /// Re-scope every row tagged `from` to `to` - every node and edge, and every row of each
    /// [`PROJECT_KEYED_LEDGERS`] table the file holds - in ONE transaction, returning the number
    /// of NODES moved. Moving the ledgers with the facts keeps the migrated identity's current
    /// generation and its assertions live under `to`, so its next generation supersedes the one
    /// it held under `from` rather than folding as its first. A file that owes its rebuild is
    /// opened as it stands and may hold none of the ledgers, or, folded before spec 28, `nodes`
    /// and `edges` with no `project` column; each table it does not hold, or holds without that
    /// column, is skipped, since it tags no row to move and the rebuild the file owes refolds it
    /// under the current identity.
    /// This is the graph analog of [`Store::rename_stream_prefix`] for
    /// the spec-09 identity migration (spec 28 GC5 backward-compat): a single-project deployment
    /// runs under its basename identity, folding graph rows tagged with it, then mints
    /// `.rigger/project.id`; the migration renames its event streams `proj-<legacy>-` ->
    /// `proj-<minted>-`, but because the graph folds incrementally the renamed streams are NEVER
    /// re-folded, so the pre-mint rows keep the legacy scope. Re-keying them to the minted
    /// identity keeps the read filter (criterion 2) returning that history, so the deployment
    /// behaves EXACTLY as before the mint. The SAME injected identity that namespaces the
    /// streams stays the ONE source of truth for the graph's project scope - this only re-derives
    /// it onto the pre-mint rows, never a second source of truth.
    ///
    /// The caller (the identity migration) re-keys BEFORE recording the migration decision, so
    /// the minted scope is still empty on the graph and no re-keyed table's composite key, each
    /// carrying `project`, collides. A failed update in any table fails the whole migration with
    /// its error and moves nothing. A no-op returning 0 when nothing is tagged `from`, so a
    /// re-open after the migration re-keys nothing (idempotent, mirroring `rename_stream_prefix`).
    pub fn migrate_project(&self, from: &str, to: &str) -> Result<usize, Error> {
        self.transact(|tx| {
            // Every table re-keys in the same transaction so a crash never leaves an edge under
            // the old scope while its endpoint node moved, or a generation behind its facts, on a
            // shared backend.
            // A table the file lacks, or holds without its `project` column, has no row to move.
            let rekey = |table: &str| {
                column_exists(tx, table, "project").and_then(|keyed| match keyed {
                    true => tx
                        .execute(
                            &format!("UPDATE {table} SET project = ?2 WHERE project = ?1"),
                            params![from, to],
                        )
                        .map_err(be),
                    false => Ok(0),
                })
            };
            rekey("edges")?;
            let nodes = rekey("nodes")?;
            for table in PROJECT_KEYED_LEDGERS {
                rekey(table)?;
            }
            Ok::<_, Error>(nodes)
        })
        .map_err(FrameError::unframed)
    }
}

/// The run-closure prune [`Projector::prune`] applies, inside the caller's transaction `tx` on the
/// graph of `project`: the one body `rigger reset --runs` and a rebuild's pruned copy both prune
/// through.
fn prune_in(
    tx: &Transaction,
    project: &str,
    node_ids: &[String],
    superseded_before: Option<i64>,
) -> Result<PruneStats, Error> {
    let ids_json = serde_json::to_string(node_ids).map_err(be)?;
    // Delete every edge referencing a pruned node from EITHER end - a superseded
    // decision's DECIDED/GOVERNS/SUPERSEDES edges and a finding's ABOUT/RAISED edges,
    // whether currently valid or already invalidated - so no edge dangles to a gone node.
    // Scoped to THIS project (spec 28 criterion 3): on a shared backend another project may
    // hold an edge that shares a from_id/to_id with a pruned id, and `reset --runs` must
    // never touch it - the SAME injected project the fold stamps is the ONE scope. An empty
    // `node_ids` yields `[]`, so `json_each` matches nothing and this is a 0-row no-op - the
    // superseded-edge reclamation below still runs.
    tx.execute(
        "DELETE FROM edges
         WHERE (from_id IN (SELECT value FROM json_each(?1))
             OR to_id IN (SELECT value FROM json_each(?1)))
           AND project = ?2",
        params![ids_json, project],
    )
    .map_err(be)?;
    // Scoped identically: the composite (id, project) key lets the SAME id live under many
    // projects, so pruning project P's dead-run node leaves project Q's same-id node intact.
    let nodes = tx
        .execute(
            "DELETE FROM nodes
             WHERE id IN (SELECT value FROM json_each(?1)) AND project = ?2",
            params![ids_json, project],
        )
        .map_err(be)?;
    // Spec 41: reclaim superseded structural edges retired before the retention boundary
    // ([`superseded_edges_sql`]). `valid_to < ?1` makes this LIVE-safe by construction - a live
    // edge's NULL `valid_to` is before no boundary, so it is never matched, so grounding, blast-radius, and the two-view safe superset are untouched. STRICT
    // `<`: an edge whose `valid_to` equals the boundary was retired at the instant the active
    // run began, so it is recent history the window keeps, not cumulative cruft. Project-scoped
    // like the node deletes and in this same transaction, so the whole prune is atomic. `None`
    // reclaims nothing (a legacy store with no run boundary, or a node-only prune).
    let superseded_edges = match superseded_before {
        Some(before) => tx
            .execute(
                &superseded_edges_sql("DELETE", ""),
                params![before, project],
            )
            .map_err(be)?,
        None => 0,
    };
    Ok(PruneStats {
        nodes,
        superseded_edges,
    })
}

/// Run `body` in ONE transaction on `conn` and commit it: the one IMMEDIATE transaction -> body ->
/// commit frame of this file; [`open_current`] keeps its own immediate transaction
/// on the shared connection it is handed. The open takes the write lock (spec 122), so a frame
/// meeting a lock another connection holds fails at its open once the busy timeout passes, before
/// `body` runs - [`FrameError::Contended`] for SQLite's busy or locked code, read there before
/// anything stringifies it, and [`FrameError::Failed`] for any other open error (spec 124) - and a
/// `body` that reads before it writes never finds its read outdated by another's commit. A `body`
/// that fails - with the domain's error, which is [`FrameError::Failed`], or a frame error of its
/// own - or a commit that does, leaves nothing written - the transaction rolls back whole as it drops.
fn in_transaction<T, E>(
    conn: &mut Connection,
    body: impl FnOnce(&Transaction) -> Result<T, E>,
) -> Result<T, FrameError>
where
    FrameError: From<E>,
{
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| match e.sqlite_error_code() {
            Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => {
                FrameError::Contended(e)
            }
            _ => FrameError::Failed(be(e)),
        })?;
    let answer = body(&tx)?;
    tx.commit().map_err(be)?;
    Ok(answer)
}

/// Apply to the pruned copy behind `conn` the run-closure prune `rigger reset --runs` applies
/// (spec 21, spec 101): the drop set and the superseded-edge boundary derived, through the one
/// rule ([`rigger_domain::run::superseded_graph_nodes`], [`rigger_domain::run::superseded_edge_boundary`]),
/// from the WHOLE run attribution the shadow fold gathered ([`REBUILD_STATE`]), pruned through the
/// one prune body ([`prune_in`]) - so the rebuilt graph never resurrects a node a prune dropped.
/// The copy is always of an unpruned shadow, so this is the one prune of the whole live selection
/// however many passes the rebuild took, and what it removed is stamped, in its transaction, as
/// the counts the rebuild state records - the cold rebuild's counts, never a sum.
fn prune_run_closure(conn: &mut Connection, project: &str) -> Result<(), Error> {
    in_transaction(conn, |tx| -> Result<(), Error> {
        let gathered = tx
            .prepare(
                "SELECT position, type, data, valid_from FROM rebuild_run_closure ORDER BY position",
            )
            .map_err(be)?
            .query_map([], |r| {
                let mut e = Event::new(&r.get::<_, String>(1)?, r.get(2)?);
                e.position = r.get::<_, i64>(0)? as Position;
                e.valid_from = from_nanos(r.get(3)?);
                Ok(e)
            })
            .map_err(be)?
            .collect::<Result<Vec<Event>, _>>()
            .map_err(be)?;
        let pruned = prune_in(
            tx,
            project,
            &rigger_domain::run::superseded_graph_nodes(&gathered),
            rigger_domain::run::superseded_edge_boundary(&gathered),
        )?;
        tx.execute(
            "UPDATE rebuild_cursor SET pruned_nodes = ?1, reclaimed_edges = ?2",
            params![pruned.nodes as i64, pruned.superseded_edges as i64],
        )
        .map_err(be)?;
        Ok(())
    })
    .map_err(FrameError::unframed)
}

/// The private copy a [`Projector::rebuild`] of the graph file at `path` prunes and swaps in: it
/// exists only between the start and the end of one swap, and one an interrupted swap left is
/// stale - removed by the next rebuild ([`prune_a_copy`]) and by `rigger reset`
/// ([`Projector::forget_stale_copy`]), never resumed or read.
pub fn pruned_copy(path: &str) -> String {
    format!("{path}.pruned")
}

/// The rebuild lock of the graph file at `path` ([`Projector::lock_rebuild`]), taken without
/// waiting: `None` while another holds it. The lock file `<path>.lock` is created if absent, never
/// truncated, and any failure to open or lock it is reported naming it ([`failed_at`]).
fn take_rebuild_lock(path: &str) -> Result<Option<RebuildLock>, Error> {
    let lock = format!("{path}.lock");
    let held = HeldLock::try_take(Path::new(&lock)).map_err(|e| Error(failed_at(&lock, e)))?;
    Ok(held.map(|locked| RebuildLock {
        path: path.to_string(),
        _locked: locked,
    }))
}

/// The shadow file a [`Projector::rebuild`] of the graph file at `path` folds into and resumes.
fn shadow_of(path: &str) -> String {
    format!("{path}.rebuild")
}

/// Whether the shadow of a rebuild of the graph file at `path` stands beside it ([`shadow_of`]):
/// read without opening it.
fn shadow_stands(path: &str) -> bool {
    Path::new(&shadow_of(path)).exists()
}

/// Whether the standing shadow behind `shadow` can be resumed by a rebuild folding the log whose
/// identity is `store`: it holds the run attribution the rebuild gathered, from which the prune is
/// derived whole, records the current fold rule ([`PROJECTION_VERSION`]), under which it was
/// folded, and was folded from that log - its `caught_up` cursor recorded beside it, or carrying
/// none, as a binary before the cursor left it, which the shadow open creates ([`create_cursor`]).
/// A shadow without that attribution - one whose rebuild state was never opened, or was torn -
/// folded under another rule (a rule-1 binary recorded none), or whose cursor was recorded beside
/// another log or beside none ([`log_switched`], spec 124, A LOG SWITCHED UNDER A STANDING GRAPH)
/// is not.
fn shadow_resumable(shadow: &Connection, store: &str) -> Result<bool, Error> {
    Ok(has_table(shadow, "rebuild_run_closure")?
        && fold_rule(shadow)? == PROJECTION_VERSION
        && caught_up_beside(shadow)?
            .is_none_or(|(_, recorded)| !log_switched(recorded.as_deref(), store)))
}

/// What a [`Projector::rebuild`] of a graph file pays with its shadow (spec 124).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShadowPay {
    /// A fresh shadow, the log folded into it from its start.
    Fresh,
    /// The standing shadow, resumed from its last committed transaction.
    Resume,
    /// No shadow: the pay finishes a standing tail, or owes nothing.
    Skip,
}

impl ShadowPay {
    /// Whether this pay starts a fresh shadow and folds the log into it from its start.
    fn starts_fresh(self) -> bool {
        self == ShadowPay::Fresh
    }
}

/// What a rebuild of the graph file at `path`, live on `live`, owed by its caller when `owed` and
/// folding the log whose identity is `store`, pays with its shadow: a standing shadow
/// ([`shadow_stands`]) is resumed when it is resumable from that log ([`shadow_resumable`]) and
/// replaced by a fresh one otherwise; with none standing, a fresh one is started when a cause is
/// owed - `owed`, or the older fold rule the file records ([`owed_by_file`]) - whether or not a
/// swapped-in tail stands, and none otherwise. A standing shadow is opened without being created
/// and only read. The one classification [`Projector::rebuild`] pays by and
/// [`Projector::rebuild_starts_fresh`] answers.
fn shadow_pay(path: &str, live: &Connection, owed: bool, store: &str) -> Result<ShadowPay, Error> {
    if shadow_stands(path) {
        let shadow = Connection::open_with_flags(
            shadow_of(path),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
        )
        .map_err(be)?;
        return Ok(if shadow_resumable(&shadow, store)? {
            ShadowPay::Resume
        } else {
            ShadowPay::Fresh
        });
    }
    Ok(if owed || !owed_by_file(live)?.is_empty() {
        ShadowPay::Fresh
    } else {
        ShadowPay::Skip
    })
}

/// Remove the file at `path`, if there is one: a file already gone is what removing it asks for,
/// and any other failure is reported naming the file ([`failed_at`]).
fn remove_if_present(path: impl AsRef<Path>) -> Result<(), Error> {
    match std::fs::remove_file(&path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(Error(failed_at(path, e))),
        _ => Ok(()),
    }
}

/// The one spelling of a filesystem failure on the file at `path`: `<path>: <io error>`.
fn failed_at(path: impl AsRef<Path>, e: std::io::Error) -> String {
    format!("{}: {e}", path.as_ref().display())
}

/// Derive the graph a rebuild swaps in from the shadow behind `shadow`, which stays unpruned: a
/// stale copy at `copy` is removed, the shadow is written to a fresh one there (`VACUUM INTO`),
/// that copy is pruned from the whole gathered attribution ([`prune_run_closure`]) and stamped
/// with the current [`PROJECTION_VERSION`], and the open copy is handed back for the swap. The
/// caller holds the rebuild lock throughout ([`RebuildLock`]), so no other rebuild, and no `rigger
/// reset`, meets the copy; the copy is opened without being created, so one removed under it is an
/// error, never an empty graph.
fn prune_a_copy(shadow: &Connection, copy: &str, project: &str) -> Result<Connection, Error> {
    remove_if_present(copy)?;
    shadow.execute("VACUUM INTO ?1", [copy]).map_err(be)?;
    let mut pruned = Connection::open_with_flags(copy, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(be)?;
    prune_run_closure(&mut pruned, project)?;
    pruned
        .pragma_update(None, "user_version", PROJECTION_VERSION)
        .map_err(be)?;
    Ok(pruned)
}

/// The fold rule that folds the records alone ([`PROJECTION_VERSION`] 7).
const RECORDS_ONLY_RULE: i64 = 7;

/// The node kinds a record event folds, whose nodes, edges and assertions a migration to the
/// records-only rule keeps ([`refuse_a_lossy_refold`]).
const RECORD_KINDS: [&str; 3] = [KIND_DECISION, KIND_FINDING, KIND_LESSON];

/// Refuse the swap of the records-only refold the shadow behind `shadow` holds for `project` over
/// the graph file at `live_path`, which the rule before it folded (addendum section 11, steps 3 and
/// 4): that rule folded the records exactly as this one does, so every record fact the old file
/// holds - each record node with its attrs, each live or retired edge from or onto one (its
/// endpoints, relation, validity and tier, endpoints compared by id, so a governed file the old
/// file held as a file is kept as the refold's bare artifact), each assertion the record identity
/// holds, and each alias - must be in the refold, and no record may name a grouping id, which no
/// fold holds any longer ([`grouping_id_sql`]). A grouping's memberships, and the record identity's
/// assertion of a grouping, are the grouping passes' rows rather than record facts and are not
/// compared. Anything missing, or any record naming a grouping, is an error printing each one,
/// returned before the swap, so the old file stands untouched and the shadow stays standing for a
/// rerun to compare and refuse again. Only the old file's columns are compared: a file an older
/// shape left may lack one.
fn refuse_a_lossy_refold(shadow: &Connection, live_path: &str, project: &str) -> Result<(), Error> {
    shadow
        .execute("ATTACH DATABASE ?1 AS old", [live_path])
        .map_err(be)?;
    let lost = record_facts_lost(shadow, project);
    shadow.execute_batch("DETACH DATABASE old").map_err(be)?;
    let lost = lost?;
    let groupings = labels(
        shadow,
        &format!(
            "SELECT 'edge ' || from_id || ' -' || rel || '-> ' || to_id FROM edges
              WHERE project = {p} AND from_id IN (SELECT id FROM nodes WHERE project = {p}
                                                   AND kind IN ({kinds}))
                AND {grouping}",
            p = sql_text(project),
            kinds = record_kinds_sql(),
            grouping = grouping_id_sql("to_id"),
        ),
    )?;
    if lost.is_empty() && groupings.is_empty() {
        return Ok(());
    }
    let mut refusal = vec![format!(
        "{live_path} was not migrated to the records-only fold and is kept as it stands:"
    )];
    for (heading, lines) in [
        (
            "record facts it holds that the records refold to without:",
            lost,
        ),
        (
            "records naming a grouping id, which no fold holds:",
            groupings,
        ),
    ] {
        if !lines.is_empty() {
            refusal.push(heading.to_string());
            refusal.extend(lines.into_iter().map(|line| format!("  {line}")));
        }
    }
    Err(Error(refusal.join("\n")))
}

/// One table [`record_facts_lost`] compares: its name, the columns that name a fact, the columns
/// compared beside them where the old file has them, the condition a record fact meets, and the
/// SQL that labels a fact.
type RecordTable<'a> = (&'a str, &'a [&'a str], &'a [&'a str], String, String);

/// Every record fact the file attached as `old` holds for `project` that the main file lacks
/// ([`refuse_a_lossy_refold`]), one line each: its record nodes, the edges from or onto them, the
/// record identity's assertions and the aliases, each compared on the columns the old file has
/// of those it names, and scoped to `project` in each file whose table carries one.
fn record_facts_lost(conn: &Connection, project: &str) -> Result<Vec<String>, Error> {
    let p = sql_text(project);
    let kinds = record_kinds_sql();
    let old_records = format!("SELECT id FROM old.nodes WHERE kind IN ({kinds})");
    let tables: [RecordTable; 4] = [
        (
            "nodes",
            &["id", "kind"],
            &["attrs"],
            format!("kind IN ({kinds})"),
            "'node ' || id || ' (' || kind || ')'".to_string(),
        ),
        (
            "edges",
            &["from_id", "to_id", "rel"],
            &["valid_from", "valid_to", "tier"],
            format!(
                "(from_id IN ({old_records}) OR to_id IN ({old_records})) AND NOT {} AND NOT {}",
                grouping_id_sql("from_id"),
                grouping_id_sql("to_id")
            ),
            "'edge ' || from_id || ' -' || rel || '-> ' || to_id".to_string(),
        ),
        (
            "node_assertions",
            &["node_id", "kind"],
            &["attrs", "generation"],
            format!("identity = '' AND NOT {}", grouping_id_sql("node_id")),
            "'assertion of ' || node_id || ' (' || kind || ')'".to_string(),
        ),
        (
            "aliases",
            &["alias", "canonical_id"],
            &[],
            "1".to_string(),
            "'alias ' || alias || ' -> ' || canonical_id".to_string(),
        ),
    ];
    let mut lost = Vec::new();
    for (table, named, compared, filter, label) in tables {
        let columns = columns_of(conn, "old", table)?;
        if !named.iter().all(|c| columns.contains(*c)) {
            continue;
        }
        let scoped = |schema: &str, keyed: bool| {
            let scope = if keyed {
                format!("project = {p} AND ")
            } else {
                String::new()
            };
            format!("SELECT {{compared}} FROM {schema}.{table} WHERE {scope}{filter}")
        };
        let (old, new) = (
            scoped("old", columns.contains("project")),
            scoped("main", columns_of(conn, "main", table)?.contains("project")),
        );
        let compared: Vec<&str> = named
            .iter()
            .chain(compared)
            .copied()
            .filter(|c| columns.contains(*c))
            .collect();
        let compared = compared.join(", ");
        lost.extend(labels(
            conn,
            &format!("SELECT {label} FROM ({old} EXCEPT {new})").replace("{compared}", &compared),
        )?);
    }
    Ok(lost)
}

/// The columns the file `conn` holds as `schema` gives `table`: none when it holds no such table.
fn columns_of(conn: &Connection, schema: &str, table: &str) -> Result<BTreeSet<String>, Error> {
    let mut stmt = conn
        .prepare("SELECT name FROM pragma_table_info(?1, ?2)")
        .map_err(be)?;
    let names = stmt
        .query_map([table, schema], |r| r.get(0))
        .map_err(be)?
        .collect::<Result<_, _>>()
        .map_err(be);
    names
}

/// The distinct one-column text rows `sql` selects, sorted.
fn labels(conn: &Connection, sql: &str) -> Result<Vec<String>, Error> {
    let mut stmt = conn
        .prepare(&format!("SELECT DISTINCT * FROM ({sql}) ORDER BY 1"))
        .map_err(be)?;
    let rows = stmt
        .query_map([], |r| r.get(0))
        .map_err(be)?
        .collect::<Result<_, _>>()
        .map_err(be);
    rows
}

/// [`RECORD_KINDS`] as a SQL list of text literals.
fn record_kinds_sql() -> String {
    RECORD_KINDS.map(sql_text).join(", ")
}

/// The SQL term the id `column` holds passes when it names a grouping: a community or a concept,
/// each id beginning with its pass's prefix.
fn grouping_id_sql(column: &str) -> String {
    let prefixed = |prefix: &str| {
        format!(
            "substr({column}, 1, {}) = {}",
            prefix.len(),
            sql_text(prefix)
        )
    };
    format!(
        "({} OR {})",
        prefixed(rigger_domain::community::COMMUNITY_ID_PREFIX),
        prefixed(rigger_domain::concepts::CONCEPT_ID_PREFIX)
    )
}

/// `text` as a SQL text literal, each quote doubled.
fn sql_text(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// Open the rebuild's shadow file at `path`, creating it when there is none, and start it afresh
/// when `fresh` says the pay starts a fresh shadow ([`shadow_pay`]). Only the holder of the rebuild
/// lock ([`RebuildLock`]) calls this, so the shadow it opens is its own: no other rebuild has it
/// open, waits on it or removes it, and none can take its name while this one holds it. A shadow is
/// resumed only together with the run attribution it gathered, and only under the fold rule it was
/// folded under ([`shadow_resumable`]): one without that attribution could derive the prune from
/// part of it alone, and one of another [`PROJECTION_VERSION`] holds what that rule folded, so a
/// fresh pay removes a standing one ([`remove_if_present`]) before it opens the shadow empty, to be
/// folded from the start - under the lock, like every other end of a shadow. Its caller's
/// classification read the standing shadow first, which rolls back a journal a killed rebuild left
/// hot, so the discarded one leaves none for the next shadow of its name to pair with. The shadow
/// handed back records the current rule.
fn open_shadow(path: &str, fresh: bool) -> Result<Connection, Error> {
    if fresh {
        remove_if_present(path)?;
    }
    let shadow = Connection::open(path).map_err(be)?;
    shadow
        .pragma_update(None, "user_version", PROJECTION_VERSION)
        .map_err(be)?;
    Ok(shadow)
}

/// End the shadow at `path` that `shadow` holds: the connection is closed first, its journal with
/// it, and only then is the file removed. The rebuild lock's holder is the shadow's sole owner, so
/// no other connection holds the file it removes, no foreign journal pairs with its name, and no
/// connection to a removed shadow is left while a newer shadow could take that name.
fn remove_shadow(shadow: Connection, path: &str) -> Result<(), Error> {
    drop(shadow);
    remove_if_present(path)
}

/// Fold what `source` hands after the cursor of `conn`'s [`REBUILD_STATE`], each batch in the
/// transactions [`fold_transactions`] cuts it into (spec 122, ONE BATCHING RULE) - each committed
/// with its last position as the cursor, so a writer meeting the pass waits at most one of them and
/// none spans two batches - reporting each batch to `progress` once its last transaction commits.
/// Each transaction is decided and folded by THE DECIDE STEP ([`decide_step`], spec 124), so a
/// position it passes over is recorded with its cause in the file's `passed_over` table; any
/// other failure propagates with its transaction rolled back, so nothing records the event folded
/// and the next rebuild resumes from the cursor - the last position the transaction before it
/// committed - and folds it. A rebuild pass meeting another writer's lock past the busy timeout
/// fails the same way, with the error's own text ([`FrameError::unframed`]).
///
/// A shadow fold (`gather_run_closure`) also keeps, in the same transaction, every event of the
/// [`rigger_domain::run::RUN_CLOSURE_TYPES`] it passes ([`REBUILD_STATE`]), whatever became
/// of its fold, so the run-closure prune is re-derived from the same ordered pass.
fn fold_source(
    conn: &mut Connection,
    project: &str,
    source: &mut RebuildSource,
    progress: &mut dyn FnMut(RebuildProgress),
    gather_run_closure: bool,
) -> Result<(), Error> {
    let start = rebuild_cursor(conn)?;
    let mut folded = 0;
    source(start, &mut |events, head| {
        let mut through = start;
        for part in fold_transactions(events) {
            through = part.last().map_or(start, |e| e.position);
            decide_step(
                conn,
                project,
                part,
                through,
                &mut RebuildCursor,
                gather_run_closure,
            )
            .map_err(FrameError::unframed)?;
        }
        folded += events.len();
        progress(RebuildProgress {
            start,
            through,
            head,
            folded,
        });
        Ok(())
    })
}

/// THE DECIDE STEP (spec 124): fold the one transaction cut `part` of a fold pass
/// ([`fold_transactions`]), none for a catch-up page of other streams' events alone, into the
/// connection `conn` lends, deciding each position in order, in a transaction that `cursor` opens
/// before any position folds and, once they are folded, closes at `through` ([`FoldCursor`]): a
/// rebuild's fold pass ([`fold_source`]) advancing its `rebuild_cursor` to the cut's last position
/// ([`RebuildCursor`]), a catch-up ([`Projector::catch_up`]) reading its `caught_up` cursor again
/// under the write lock and advancing it ([`CatchUpCut`]); a cursor that declines its open folds
/// nothing, answering its decline. A position already in the `applied` ledger, passed over or
/// folded, is passed by the fold's per-position guard. Each event folds as [`RecordFold::apply`]
/// folds it, from its own bytes, under a savepoint of its own ([`fold_in_savepoint`]); one whose
/// own payload the judge ([`super::check_fold_payload`] - deterministic, so no fold will ever hold
/// it) rejects is passed over with the class [`PAYLOAD_REJECTED`] (CONTENT). Any other failure
/// (ENVIRONMENT) - a fold's own error over a payload the judge accepts; a statement outside every
/// fold - propagates with the cut's transaction rolled back, and an open meeting another writer's
/// lock past the busy timeout answers [`FrameError::Contended`] (CONTENTION), so nothing is passed
/// over but CONTENT.
fn decide_step<L: Lend>(
    conn: &mut L,
    project: &str,
    part: &[Event],
    through: Position,
    cursor: &mut impl FoldCursor,
    gather_run_closure: bool,
) -> Result<(), FrameError> {
    conn.lend(|conn| {
        in_transaction(conn, |tx| {
            cursor.open(tx, through)?;
            for e in part {
                fold_in_savepoint(tx, e, gather_run_closure, || {
                    fold_new(tx, std::slice::from_ref(e), project).map(drop)
                })?;
            }
            cursor.close(tx, through)
        })
    })
}

/// Fold the event `e` of a fold pass ([`decide_step`]) through `fold`, in `tx`, under a savepoint
/// of its own. One whose `fold` fails while the judge ([`super::check_fold_payload`]) rejects its
/// own payload rolls the savepoint back and is recorded passed over with that cause
/// ([`PAYLOAD_REJECTED`], [`pass_over`]); any other failure of `fold` propagates. A folded event of
/// the [`rigger_domain::run::RUN_CLOSURE_TYPES`] is kept for the run-closure prune when
/// `gather_run_closure` asks it ([`REBUILD_STATE`]).
fn fold_in_savepoint(
    tx: &Transaction,
    e: &Event,
    gather_run_closure: bool,
    fold: impl FnOnce() -> Result<(), Error>,
) -> Result<(), Error> {
    tx.execute_batch("SAVEPOINT fold_event").map_err(be)?;
    if let Err(failed) = fold() {
        let cause = rejection(PAYLOAD_REJECTED, &e.type_, &e.data).ok_or(failed)?;
        tx.execute_batch("ROLLBACK TO fold_event").map_err(be)?;
        pass_over(tx, e.position, &cause)?;
    }
    tx.execute_batch("RELEASE fold_event").map_err(be)?;
    if gather_run_closure && rigger_domain::run::RUN_CLOSURE_TYPES.contains(&e.type_.as_str()) {
        tx.execute(
            "INSERT OR IGNORE INTO rebuild_run_closure
             (position, type, data, valid_from) VALUES (?1, ?2, ?3, ?4)",
            params![e.position as i64, e.type_, e.data, to_nanos(e.valid_from)],
        )
        .map_err(be)?;
    }
    Ok(())
}

/// Record `position` passed over with `cause` in `tx` (spec 124, PASSED OVER WITH ITS CAUSE): its
/// row in the `applied` ledger, so no fold folds it again and no rebuild is owed for it, and its
/// row in `passed_over`, written together in one transaction.
fn pass_over(tx: &Transaction, position: Position, cause: &str) -> Result<(), Error> {
    record_applied(tx, position)?;
    tx.execute(
        "INSERT INTO passed_over (position, cause) VALUES (?1, ?2)",
        params![position as i64, cause],
    )
    .map(drop)
    .map_err(be)
}

/// The cause a file recorded the first of the positions the JSON array `?1` names passed over
/// with ([`pass_over`]), or none: the one read of an own position passed over, which a direct fold
/// ([`passed_over_cause`]) and an appender's success test ([`own_standing`]) share (spec 124).
const FIRST_PASSED_OVER: &str = "SELECT cause FROM passed_over \
     WHERE position IN (SELECT value FROM json_each(?1)) ORDER BY position LIMIT 1";

/// The cause `conn`'s file recorded the first of `positions` passed over with
/// ([`FIRST_PASSED_OVER`]), or none.
fn passed_over_cause(conn: &Connection, positions: &[Position]) -> Result<Option<String>, Error> {
    let positions = serde_json::to_string(positions).map_err(be)?;
    conn.query_row(FIRST_PASSED_OVER, [positions], |r| r.get(0))
        .optional()
        .map_err(be)
}

/// Where an appender of the positions the JSON array `positions` names stands (spec 124, THE
/// APPENDER's success test): the `caught_up` cursor of the file behind `conn` and the cause the
/// first of them was recorded passed over with ([`FIRST_PASSED_OVER`]), read together in ONE
/// statement outside any transaction ([`cursor_row`]) - a row is written in the transaction that
/// moves the cursor past its position, so a read that sees the cursor past it sees the row - or
/// none on a file without the cursor.
fn own_standing(
    conn: &Connection,
    positions: &str,
) -> Result<Option<(Position, Option<String>)>, Error> {
    cursor_row(
        conn,
        CAUGHT_UP,
        &format!(", ({FIRST_PASSED_OVER})"),
        [positions],
        |cursor, r| Ok((cursor, r.get(1)?)),
    )
}

/// Whether `e` is an event of the project's run stream (spec 124, ONE PROCEDURE): the stream a
/// catch-up folds from the log ([`Projector::catch_up`]), and so the stream an appender's events
/// must all be of for its calls to fold them ([`Projector::folds_by_calls`]).
fn on_the_run_stream(e: &Event) -> bool {
    e.stream == rigger_domain::run::STREAM
}

/// A connection THE DECIDE STEP ([`decide_step`]) folds into, lent for one use at a time: a
/// rebuild's fold pass lends the connection it holds for the whole pass ([`Connection`]), and a
/// catch-up its projector's, locking the projector's mutex for that use alone
/// (`&Mutex<Connection>`), so between two uses the call holds no lock of the file (spec 124, ONE
/// PROCEDURE).
trait Lend {
    /// Answer what `use_` answers, handed the connection for its length.
    fn lend<T>(&mut self, use_: impl FnOnce(&mut Connection) -> T) -> T;
}

impl Lend for Connection {
    fn lend<T>(&mut self, use_: impl FnOnce(&mut Connection) -> T) -> T {
        use_(self)
    }
}

impl Lend for &Mutex<Connection> {
    fn lend<T>(&mut self, use_: impl FnOnce(&mut Connection) -> T) -> T {
        use_(&mut self.lock().unwrap())
    }
}

/// The cursor one transaction cut of THE DECIDE STEP ([`decide_step`]) answers to, handed the
/// cut's last position: asked in the cut's transaction before any position folds, and again once
/// they are folded.
trait FoldCursor {
    /// Before any position of the cut folds: an error declines the cut, which folds nothing.
    fn open(&mut self, tx: &Transaction, through: Position) -> Result<(), FrameError>;
    /// Once the cut's positions are folded: advance the cursor to `through`.
    fn close(&mut self, tx: &Transaction, through: Position) -> Result<(), FrameError>;
}

/// The cursor of a rebuild's fold pass ([`fold_source`]): the `rebuild_cursor` of its
/// [`REBUILD_STATE`], opened with no check and set at the cut's last position ([`set_cursor`]),
/// committed with the transaction that folded it.
struct RebuildCursor;

impl FoldCursor for RebuildCursor {
    fn open(&mut self, _: &Transaction, _: Position) -> Result<(), FrameError> {
        Ok(())
    }

    fn close(&mut self, tx: &Transaction, through: Position) -> Result<(), FrameError> {
        Ok(set_cursor(tx, "rebuild_cursor", through)?)
    }
}

/// The cut of one [`Projector::catch_up`] call, its page read from `from` by a projector folding
/// the log `store` names (spec 124, ONE PROCEDURE). It opens by reading the file's standing again
/// under the write lock ([`standing_cursor`]): a rebuild's state, an older fold rule, no
/// `caught_up` cursor or one recorded beside another log or none declines it as
/// [`FrameError::Inert`], and a cursor at or past the cut's last position, or below `from`, as
/// [`FrameError::Reread`] at that cursor; otherwise `lock_at` is that cursor and `before` the
/// positions the `applied` ledger records past it through the cut's last ([`applied_between`]).
/// It closes by setting `caught_up` at the cut's last position ([`set_cursor`]) and counting in
/// `recorded` the positions that ledger gained there in the cut's own transaction.
struct CatchUpCut<'s> {
    store: &'s str,
    from: Position,
    lock_at: Position,
    before: usize,
    recorded: usize,
}

impl FoldCursor for CatchUpCut<'_> {
    fn open(&mut self, tx: &Transaction, through: Position) -> Result<(), FrameError> {
        let at = standing_cursor(tx, self.store)?.map_err(FrameError::Inert)?;
        if !(self.from <= at && at < through) {
            return Err(FrameError::Reread(at));
        }
        self.lock_at = at;
        self.before = applied_between(tx, at, through)?;
        Ok(())
    }

    fn close(&mut self, tx: &Transaction, through: Position) -> Result<(), FrameError> {
        set_cursor(tx, CAUGHT_UP, through)?;
        self.recorded = applied_between(tx, self.lock_at, through)? - self.before;
        Ok(())
    }
}

/// Where the `caught_up` cursor of the file behind `conn` stands for a catch-up of its own
/// ([`Projector::catch_up`]) by a projector folding the log `store` names, or the first cause, in
/// this order, that it folds none ([`Inert`]): it holds a rebuild's state ([`rebuild_tail_owed`]),
/// it owes its rebuild for an older fold rule over what it folded ([`rebuild_owed`]), it carries no
/// cursor, or its cursor was recorded beside another log than `store`, or beside none
/// ([`log_switched`]).
fn standing_cursor(conn: &Connection, store: &str) -> Result<Result<Position, Inert>, Error> {
    if rebuild_tail_owed(conn)? {
        return Ok(Err(Inert::RebuildState));
    }
    if rebuild_owed(conn)? {
        return Ok(Err(Inert::OlderRule));
    }
    Ok(match caught_up_beside(conn)? {
        None => Err(Inert::NoCursor),
        Some((_, recorded)) if log_switched(recorded.as_deref(), store) => Err(Inert::LogSwitched),
        Some((at, _)) => Ok(at),
    })
}

/// How many positions the `applied` ledger of the file behind `conn` records past `after`, up to
/// and including `through`.
fn applied_between(conn: &Connection, after: Position, through: Position) -> Result<usize, Error> {
    conn.query_row(
        "SELECT count(*) FROM applied WHERE position > ?1 AND position <= ?2",
        [after as i64, through as i64],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n as usize)
    .map_err(be)
}

/// The most events that one transaction of a fold pass ([`fold_source`]) folds before it commits
/// (spec 122, ONE BATCHING RULE): a writer meeting the pass waits at most this many events' fold.
/// It is also the most events one
/// [`Projector::catch_up`] call reads of the log for its page ([`EventStore::read_all_page`]).
pub const FOLD_BATCH: usize = 256;

/// The transactions a fold pass ([`fold_source`]) folds `events` in, in order: slices of at most
/// [`FOLD_BATCH`], each a slice of `events`, never empty.
fn fold_transactions(events: &[Event]) -> impl Iterator<Item = &[Event]> {
    events.chunks(FOLD_BATCH)
}

/// Whether the file behind `conn` was swapped in by a [`Projector::rebuild`] that has not yet
/// folded the tail the log gained while it ran: it still holds the rebuild's cursor.
fn rebuild_tail_owed(conn: &Connection) -> Result<bool, Error> {
    has_table(conn, "rebuild_cursor")
}

/// The cursor the [`REBUILD_STATE`] of a [`Projector::rebuild`] holds in the file behind `conn`
/// ([`cursor_at`]) - the last position the rebuild folded, past which a file it swapped in owes
/// that rebuild's tail - or `None` for a file holding no rebuild state, which owes no tail.
fn rebuild_tail_from(conn: &Connection) -> Result<Option<Position>, Error> {
    cursor_at(conn, "rebuild_cursor")
}

/// Where the `caught_up` cursor of the file behind `conn` stands and the identity of the log it
/// was recorded beside ([`create_cursor`]) - none for a cursor whose table predates that identity,
/// read as it stands and never an error - or `None` on a file without the cursor ([`cursor_row`]).
fn caught_up_beside(conn: &Connection) -> Result<Option<(Position, Option<String>)>, Error> {
    cursor_row(conn, CAUGHT_UP, ", *", [], |position, r| {
        match r.get::<_, Option<String>>("store") {
            Err(rusqlite::Error::InvalidColumnName(_)) => Ok((position, None)),
            recorded => Ok((position, recorded?)),
        }
    })
}

/// Whether the log a cursor was recorded beside, `recorded` - none for a cursor whose table
/// predates that identity - is not the log `selected` its projector folds (spec 124, A LOG
/// SWITCHED UNDER A STANDING GRAPH): the event store was switched under the file, so the prefix
/// its cursor claims is of another log, or of a log it cannot name. The one comparison both reads
/// of the cursor apply: setup's seeding ([`Projector::seed_cursor`]) and every catch-up's standing
/// ([`standing_cursor`]).
fn log_switched(recorded: Option<&str>, selected: &str) -> bool {
    recorded != Some(selected)
}

/// Whether the file behind `conn` carries its `caught_up` cursor ([`cursor_at`]).
fn carries_caught_up(conn: &Connection) -> Result<bool, Error> {
    Ok(cursor_at(conn, CAUGHT_UP)?.is_some())
}

/// Where the one-row cursor table `cursor` of the file behind `conn` stands ([`cursor_row`]), or
/// `None` on a file without it.
fn cursor_at(conn: &Connection, cursor: &str) -> Result<Option<Position>, Error> {
    cursor_row(conn, cursor, "", [], |position, _| Ok(position))
}

/// The one read of a cursor table: the one row of the cursor table `cursor` of the file behind
/// `conn`, its position and the further columns `beside` of the same statement, bound to
/// `params`, answered through `row` - or `None` on a file without the table.
fn cursor_row<T>(
    conn: &Connection,
    cursor: &str,
    beside: &str,
    params: impl rusqlite::Params,
    row: impl FnOnce(Position, &rusqlite::Row) -> rusqlite::Result<T>,
) -> Result<Option<T>, Error> {
    if !has_table(conn, cursor)? {
        return Ok(None);
    }
    conn.query_row(
        &format!("SELECT position{beside} FROM {cursor}"),
        params,
        |r| row(r.get::<_, i64>(0)? as Position, r),
    )
    .map(Some)
    .map_err(be)
}

/// The last position the rebuild whose [`REBUILD_STATE`] the file behind `conn` holds folded
/// ([`rebuild_tail_from`]), or the error saying the file holds no rebuild state.
fn rebuild_cursor(conn: &Connection) -> Result<Position, Error> {
    rebuild_tail_from(conn)?
        .ok_or_else(|| Error("the graph file holds no rebuild state".to_string()))
}

/// Create the `caught_up` cursor in `tx` where its file carries none, seeded at `at` beside the
/// identity `store` of the log it folds (spec 124, THE CURSOR AND ITS SEEDING, A LOG SWITCHED
/// UNDER A STANDING GRAPH): how far the file's prefix of that log reaches, every live position at
/// or below it applied or passed over. One row, its id pinned to 0, written by `INSERT OR IGNORE`,
/// so a file already carrying the cursor keeps where it stands and the identity it records. What
/// the seeding ([`Projector::seed_cursor`]) and a rebuild's shadow open and drop
/// ([`Projector::rebuild`]) create it through.
fn create_cursor(tx: &Transaction, at: Position, store: &str) -> Result<(), Error> {
    tx.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS {CAUGHT_UP} (
            id INTEGER PRIMARY KEY CHECK (id = 0),
            position INTEGER NOT NULL,
            store TEXT
        )"
    ))
    .map_err(be)?;
    tx.execute(
        &format!("INSERT OR IGNORE INTO {CAUGHT_UP} (id, position, store) VALUES (0, ?1, ?2)"),
        rusqlite::params![at as i64, store],
    )
    .map(drop)
    .map_err(be)
}

/// Set the one-row cursor table `cursor` of the file in `tx` at `at` where it stands below `at`:
/// the one statement that moves where a cursor stands, never backward - a cursor at or above `at`
/// keeps where it stands (spec 124, ONE PROCEDURE). The rebuild state's `rebuild_cursor`
/// ([`REBUILD_STATE`]) is set at the last position a fold pass ([`fold_source`]) folded, committed
/// with the transaction that folded it ([`RebuildCursor`]); the `caught_up` cursor, on a file
/// carrying it ([`create_cursor`]), is set by a rebuild's drop ([`Projector::rebuild`]) at the last
/// position the rebuild folded (spec 124, THE CURSOR AND ITS SEEDING), and by a catch-up's cut at
/// the last position it read up to ([`CatchUpCut`]).
fn set_cursor(tx: &Transaction, cursor: &str, at: Position) -> Result<(), Error> {
    tx.execute(
        &format!("UPDATE {cursor} SET position = max(position, ?1)"),
        [at as i64],
    )
    .map(drop)
    .map_err(be)
}

/// Whether the file behind `conn` holds a table named `name`.
fn has_table(conn: &Connection, name: &str) -> Result<bool, Error> {
    conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [name],
        |r| r.get(0),
    )
    .map_err(be)
}

/// The fold rule `conn`'s file records ([`PROJECTION_VERSION`]): 0 for a file that records none.
fn fold_rule(conn: &Connection) -> Result<i64, Error> {
    conn.pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(be)
}

/// Whether `conn`'s file records a fold rule older than [`PROJECTION_VERSION`].
fn stale(conn: &Connection) -> Result<bool, Error> {
    Ok(fold_rule(conn)? < PROJECTION_VERSION)
}

/// Whether the file behind `conn` owes its cold rebuild: it records an older fold rule AND folded
/// something under it. A file at the old rule that never folded anything holds nothing a rebuild
/// could change.
fn rebuild_owed(conn: &Connection) -> Result<bool, Error> {
    if !stale(conn)? {
        return Ok(false);
    }
    if !has_table(conn, "applied")? {
        return Ok(false);
    }
    conn.query_row("SELECT EXISTS (SELECT 1 FROM applied)", [], |r| r.get(0))
        .map_err(be)
}

/// Bring a file that owes no rebuild to the current schema. One at an older fold rule has folded
/// nothing, so it takes the current shape and version at once, in one immediate transaction that
/// reads the version again under its lock: two first opens never both drop ledgers the other has
/// begun to fill.
fn open_current(conn: &Connection, project: &str) -> Result<(), Error> {
    if !stale(conn)? {
        return schema(conn, project);
    }
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate).map_err(be)?;
    if stale(&tx)? {
        tx.execute_batch(DROP_LEDGERS).map_err(be)?;
        tx.pragma_update(None, "user_version", PROJECTION_VERSION)
            .map_err(be)?;
    }
    schema(&tx, project)?;
    tx.commit().map_err(be)
}

/// Create what [`SCHEMA`] declares and run the in-place migrations, so a file of any earlier
/// shape answers every query in the current one. A pre-spec-28 graph.db (no `project` column) is
/// migrated in place, backfilling its rows with `project`.
fn schema(conn: &Connection, project: &str) -> Result<(), Error> {
    conn.execute_batch(SCHEMA).map_err(be)?;
    migrate_project_scope(conn, project)?;
    migrate_edge_tier(conn)?;
    migrate_indexes(conn)
}

/// Additive backward-compat migration (spec 28, criterion 1). A graph.db created before the
/// project scope existed has `nodes(id, kind, attrs)` and `edges(..., source)` with no
/// `project` column. Bring it to the scoped shape WITHOUT wiping it: recreate `nodes` with the
/// composite `(id, project)` primary key (so the SAME node id can coexist across projects on a
/// shared backend) and add `project` to `edges`, BACKFILLING every existing row with the
/// opener's own identity. So an upgraded single-project graph.db reads identically once the
/// read filter (criterion 2) lands - its rows carry its own project. Idempotent: a fresh or
/// already-migrated db already has the column and both arms are skipped.
fn migrate_project_scope(conn: &Connection, project: &str) -> Result<(), Error> {
    if !column_exists(conn, "nodes", "project")? {
        // SQLite cannot alter a primary key in place, so copy the old rows through a renamed
        // table into the new composite-keyed `nodes`, stamping the opener's project on each.
        conn.execute("ALTER TABLE nodes RENAME TO nodes_pre_project", [])
            .map_err(be)?;
        conn.execute_batch(
            "CREATE TABLE nodes (
               id TEXT NOT NULL, kind TEXT NOT NULL, attrs TEXT,
               project TEXT NOT NULL DEFAULT '',
               PRIMARY KEY (id, project)
             );",
        )
        .map_err(be)?;
        conn.execute(
            "INSERT INTO nodes (id, kind, attrs, project)
             SELECT id, kind, attrs, ?1 FROM nodes_pre_project",
            params![project],
        )
        .map_err(be)?;
        conn.execute("DROP TABLE nodes_pre_project", [])
            .map_err(be)?;
    }
    if !column_exists(conn, "edges", "project")? {
        conn.execute(
            "ALTER TABLE edges ADD COLUMN project TEXT NOT NULL DEFAULT ''",
            [],
        )
        .map_err(be)?;
        conn.execute("UPDATE edges SET project = ?1", params![project])
            .map_err(be)?;
    }
    Ok(())
}

/// Additive backward-compat migration for the confidence tier (spec 29a, addendum 6.2). A graph.db
/// written before the tier existed has an `edges` table with no `tier` column. Bring it to the
/// tiered shape WITHOUT wiping it: add the column, defaulting every existing row to
/// [`TIER_EXTRACTED`]. That default IS the correct backfill - every pre-29a edge is a dev-loop fact
/// (DECIDED / GOVERNS / ABOUT / SUPERSEDES / ...), which addendum 6.2 tags EXTRACTED - so unlike the
/// project backfill this needs no second UPDATE. Idempotent: a fresh or already-migrated db already
/// has the column (the `SCHEMA` literal carries it) and the arm is skipped. The literal must match
/// [`TIER_EXTRACTED`], which `tier_default_matches_the_extracted_const` pins.
fn migrate_edge_tier(conn: &Connection) -> Result<(), Error> {
    if !column_exists(conn, "edges", "tier")? {
        conn.execute(
            "ALTER TABLE edges ADD COLUMN tier TEXT NOT NULL DEFAULT 'extracted'",
            [],
        )
        .map_err(be)?;
    }
    Ok(())
}

/// Additive read-path indexes (spec 45, unit 4). Pure additions - no column, no data migration,
/// no result change - that keep the whole-graph and directed-call reads sub-linear as a repository
/// grows. Run AFTER [`migrate_project_scope`] and [`migrate_edge_tier`] so every index lands on the
/// FINAL table shapes: on a pre-spec-28 graph.db the scope migration DROPS and recreates
/// `nodes`, which would strand a `nodes` index created any earlier, so this must fire last. Unlike
/// the column migrations, `CREATE INDEX IF NOT EXISTS` is inherently idempotent, so it needs no
/// [`column_exists`] guard - a fresh db and an existing one both gain every index on open, and a
/// re-open is a no-op.
///
/// - `idx_edges_live_rel_from` is a PARTIAL index on `(rel, from_id) WHERE valid_to IS NULL`: the
///   relationship-scoped forward scan a directed CALLS/REFERENCES traversal (spec 46) walks reads
///   only LIVE edges, so restricting the index to `valid_to IS NULL` keeps it small and lets SQLite
///   seek by `rel` then `from_id` instead of scanning every historical edge. A query MUST carry the
///   exact `valid_to IS NULL` term for SQLite to use a partial index.
/// - `idx_nodes_leaf` is the LEAF index (spec 119) on `(project, <leaf>, id)`, `<leaf>` the pinned
///   [`node_leaf_sql`] expression: the node side of a leaf's nodes ([`Neighbors::leaf_nodes`])
///   seeks it, the `id` column serving a file's `<file>::` id range ([`Neighbors::members`]). SQLite uses an expression index only when the query's expression
///   MATCHES the indexed one, so every reader embeds [`node_leaf_sql`] verbatim or it silently
///   misses the index.
/// - `idx_edges_live_to_name_suffix` is a PARTIAL EXPRESSION index on a live edge's target text,
///   the pinned [`edge_text_sql`] expression, which every edge-side reader seeks by binding a leaf
///   ([`edge_binds_leaf_sql`], embedding the same expression, so index and readers cannot drift).
///
/// The node name-suffix index `idx_nodes_name_suffix` an older open created on
/// `substr(id, instr(id, '::') + 2)` is dropped: no reader keys a node by its id's rest since every
/// node-side reader keys it by its leaf (spec 119).
fn migrate_indexes(conn: &Connection) -> Result<(), Error> {
    conn.execute_batch(&format!(
        "CREATE INDEX IF NOT EXISTS idx_edges_live_rel_from
             ON edges(rel, from_id) WHERE valid_to IS NULL;
         DROP INDEX IF EXISTS idx_nodes_name_suffix;
         CREATE INDEX IF NOT EXISTS idx_nodes_leaf ON nodes(project, {}, id);
         CREATE INDEX IF NOT EXISTS idx_edges_live_to_name_suffix
             ON edges({}) WHERE valid_to IS NULL;",
        node_leaf_sql(),
        edge_text_sql("to_id")
    ))
    .map_err(be)
}

/// Whether `table` (a trusted schema-literal name, never caller input) has a column named
/// `col`, via `PRAGMA table_info` - so [`migrate_project_scope`] fires exactly once and leaves
/// a fresh or already-migrated db untouched. A table the file does not hold has no column:
/// `PRAGMA table_info` answers it with no row, so [`Projector::migrate_project`] skips a table
/// absent and one without its `project` column through this one check.
fn column_exists(conn: &Connection, table: &str, col: &str) -> Result<bool, Error> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(be)?;
    let mut rows = stmt.query([]).map_err(be)?;
    while let Some(row) = rows.next().map_err(be)? {
        let name: String = row.get(1).map_err(be)?;
        if name == col {
            return Ok(true);
        }
    }
    Ok(false)
}

fn be<E: std::fmt::Display>(e: E) -> Error {
    Error(e.to_string())
}

impl RecordFold for Projector {
    fn apply(&self, e: &Event, access: FoldAccess) -> Result<(), Error> {
        self.apply_batch(std::slice::from_ref(e), access)
    }

    /// Whether [`Projector::owed_because`] names any cause.
    fn rebuild_owed(&self) -> Result<bool, Error> {
        Ok(!self.owed_because()?.is_empty())
    }

    /// Fold a whole batch of events as THE APPENDER of them ([`Projector::guarded`], spec 124):
    /// refused on a file that owes its rebuild, and a batch that does not fold returns its cause
    /// and writes no mark. The result is IDENTICAL to folding each event with
    /// [`apply`](RecordFold::apply) in order - same per-position idempotency guard (`applied`),
    /// same fold - so batching alters CADENCE only, never the graph.
    ///
    /// Folded directly, the batch folds in ONE transaction (spec 49's batched-fold cadence): the
    /// store's transaction cost is paid ONCE for the whole file batch instead of once per event,
    /// which is the load-bearing fix for a cold `graph build` whose measured throughput was
    /// transaction-cadence bound. Atomic: a fold error rolls the whole batch back (the events are
    /// still durable in the log), never a half-applied batch - save that a batch one of whose
    /// positions the file already recorded passed over still commits its other positions, and
    /// then answers the first such position's cause ([`FrameError::Recorded`]).
    ///
    /// Folded by its calls ([`Projector::fold_by_calls`]), the batch folds in the calls'
    /// transactions, one per call, each a cut of a page that may hold other writers' positions:
    /// an event whose payload the judge rejects is passed over with [`PAYLOAD_REJECTED`] while the
    /// cut's other events commit. After the first of its calls after which one of the batch's own
    /// positions stands recorded passed over, by that call or an earlier fold, the batch answers
    /// the first such position's cause at once ([`own_standing`]): the cuts its calls folded by
    /// then are committed, and the batch's events past them are left for a later fold. Any other
    /// failure rolls back only its call's cut, the cuts before it committed.
    fn apply_batch(&self, events: &[Event], _access: FoldAccess) -> Result<(), Error> {
        self.guarded(events, |tx| match fold_new(tx, events, &self.project)? {
            Some(cause) => Err(FrameError::Recorded(cause)),
            None => Ok(()),
        })
        .map(drop)
    }
}

/// The record half's reads (7.2): each scoped to `self.project`, so on a shared backend a same-id
/// row of another project is never returned (spec 28, criterion 2). The walks over them are
/// [`Composed`](rigger_domain::composed::Composed)'s, one for both halves.
impl Neighbors for Projector {
    fn node(&self, id: &str) -> Result<Option<Node>, Error> {
        node_row(&self.conn.lock().unwrap(), id, &self.project)
    }

    fn out(&self, id: &str) -> Result<Vec<Edge>, Error> {
        self.live_edges("from_id", id)
    }

    fn incoming(&self, id: &str) -> Result<Vec<Edge>, Error> {
        self.live_edges("to_id", id)
    }

    /// The nodes in `file`'s `<file>::` id range ([`id_range_sql`]) whose [`member_sql`] is
    /// `file` (THE MEMBER RULE, spec 119).
    fn members(&self, file: &str) -> Result<Vec<Node>, Error> {
        let sql = format!(
            "SELECT id, kind, attrs FROM nodes
              WHERE {} AND +project = ?1 AND {} = ?2
              ORDER BY id",
            id_range_sql("id", "?2"),
            member_sql()
        );
        self.nodes_where(&sql, file)
    }

    /// The nodes named `leaf` ([`node_leaf_sql`], a seek of `idx_nodes_leaf`) and the nodes a live
    /// edge targets whose target text binds `leaf` ([`edge_binds_leaf_sql`], THE EDGE SIDE BINDS
    /// THE LEAF): no reader keys a node by its id's rest but through an edge into it.
    fn leaf_nodes(&self, leaf: &str) -> Result<Vec<Node>, Error> {
        let sql = format!(
            "SELECT id, kind, attrs FROM nodes WHERE project = ?1 AND {} = ?2
             UNION
             SELECT id, kind, attrs FROM nodes
              WHERE project = ?1 AND id IN (
                SELECT to_id FROM edges
                 WHERE valid_to IS NULL AND project = ?1 AND {})
             ORDER BY id",
            node_leaf_sql(),
            edge_binds_leaf_sql("to_id", "?2")
        );
        self.nodes_where(&sql, leaf)
    }

    fn of_kind(&self, kind: &str) -> Result<Vec<Node>, Error> {
        self.nodes_where(
            "SELECT id, kind, attrs FROM nodes WHERE project = ?1 AND kind = ?2 ORDER BY id",
            kind,
        )
    }

    /// The record graph stages no reference rows, so it holds no unresolved leaf.
    fn unresolved(&self, _node: &Node) -> Result<Vec<String>, Error> {
        Ok(Vec::new())
    }

    /// The `aliases` table, which carries no project column and is shared.
    fn aliases(&self) -> Result<Vec<(String, String)>, Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT canonical_id, alias FROM aliases ORDER BY alias")
            .map_err(be)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(1)?, r.get(0)?)))
            .map_err(be)?;
        rows.collect::<Result<_, _>>().map_err(be)
    }

    fn whole(&self) -> Result<Graph, Error> {
        Projector::whole(self)
    }
}

impl Projector {
    /// The live edges of this project whose `column` (`from_id` or `to_id`) is `id`, ordered by
    /// their ends then relation.
    fn live_edges(&self, column: &str, id: &str) -> Result<Vec<Edge>, Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(&format!(
                "SELECT from_id, to_id, rel, valid_from, source, tier FROM edges
                  WHERE valid_to IS NULL AND project = ?1 AND {column} = ?2
                  ORDER BY from_id, to_id, rel, valid_from"
            ))
            .map_err(be)?;
        let edges = stmt
            .query_map(params![self.project, id], row_to_edge)
            .map_err(be)?
            .collect::<Result<_, _>>()
            .map_err(be);
        edges
    }

    /// The nodes `sql` reads, `?1` this project and `?2` the one value `arg`.
    fn nodes_where(&self, sql: &str, arg: &str) -> Result<Vec<Node>, Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(sql).map_err(be)?;
        let nodes = stmt
            .query_map(params![self.project, arg], row_to_node)
            .map_err(be)?
            .collect::<Result<_, _>>()
            .map_err(be);
        nodes
    }
}

fn row_to_node(r: &rusqlite::Row) -> rusqlite::Result<Node> {
    let attrs_str: Option<String> = r.get(2)?;
    let attrs = attrs_str
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    Ok(Node {
        id: r.get(0)?,
        kind: r.get(1)?,
        attrs,
    })
}

fn row_to_edge(r: &rusqlite::Row) -> rusqlite::Result<Edge> {
    Ok(Edge {
        from: r.get(0)?,
        to: r.get(1)?,
        rel: r.get(2)?,
        valid_from: r.get(3)?,
        valid_to: None,
        source: r.get::<_, i64>(4)? as Position,
        tier: r.get(5)?,
    })
}

/// Fold every event of `events` whose position has not folded yet, in order - the one
/// per-position idempotency guard (`applied`) every fold path shares - and answer the cause of the
/// first of them already recorded passed over ([`passed_over_cause`], spec 124), which a direct
/// fold answers for its own position and a fold pass, for which every position is foreign,
/// passes. A derived event ([`DERIVED_TYPES`]), which only a log an older binary wrote holds, is
/// recorded applied and folds nothing: the code view derives from the tree, never from the log.
fn fold_new(tx: &Transaction, events: &[Event], project: &str) -> Result<Option<String>, Error> {
    let mut passed_over = None;
    for e in events {
        if DERIVED_TYPES.contains(&e.type_.as_str()) {
            record_applied(tx, e.position)?;
            continue;
        }
        if record_applied(tx, e.position)? {
            fold_event(tx, e, project)?;
        } else if passed_over.is_none() {
            passed_over = passed_over_cause(tx, std::slice::from_ref(&e.position))?;
        }
    }
    Ok(passed_over)
}

/// Whether the file behind `conn` has folded `position`: its `applied` ledger holds it.
fn has_applied(conn: &Connection, position: Position) -> Result<bool, Error> {
    conn.prepare_cached("SELECT EXISTS (SELECT 1 FROM applied WHERE position = ?1)")
        .map_err(be)?
        .query_row([position as i64], |r| r.get(0))
        .map_err(be)
}

/// The highest position the `applied` ledger of the file behind `conn` records, or none for an
/// empty ledger.
fn highest_applied(conn: &Connection) -> Result<Option<Position>, Error> {
    conn.query_row("SELECT max(position) FROM applied", [], |r| {
        r.get::<_, Option<i64>>(0)
    })
    .map(|highest| highest.map(|position| position as Position))
    .map_err(be)
}

/// Record `position` in the `applied` ledger, answering whether it was not there yet.
fn record_applied(tx: &Transaction, position: Position) -> Result<bool, Error> {
    tx.execute(
        "INSERT OR IGNORE INTO applied (position) VALUES (?1)",
        [position as i64],
    )
    .map(|inserted| inserted > 0)
    .map_err(be)
}

fn fold_event(tx: &Transaction, e: &Event, project: &str) -> Result<(), Error> {
    // The edge's bi-temporal valid-time is when the fact became true (the event's
    // caller-supplied valid_from), not the ingest time.
    let at = to_nanos(e.valid_from);
    match e.type_.as_str() {
        TYPE_DECISION_MADE => {
            let d: super::DecisionMade = serde_json::from_slice(&e.data).map_err(be)?;
            ensure_node(
                tx,
                &d.id,
                KIND_DECISION,
                &[("summary", &d.summary)],
                project,
            )?;
            // De-noise (spec 43): the graph models the TARGET PROJECT, not the loop's own
            // machinery, so no fold produces a KIND_AGENT node or an agent-attribution edge.
            // The acting persona (event actor) is NOT projected - the decision's CONTENT and its
            // GOVERNS edges to the code it concerns are what the graph is about. The actor stays
            // in the log for metrics and the run-tree, which read events, not this projection.
            for path in &d.governs {
                let canonical = resolve_in_tx(tx, path);
                ensure_node(tx, &canonical, KIND_ARTIFACT, &[], project)?;
                add_edge(
                    tx,
                    &d.id,
                    &canonical,
                    REL_GOVERNS,
                    at,
                    e.position,
                    project,
                    TIER_EXTRACTED,
                )?;
            }
            if !d.supersedes.is_empty() {
                ensure_node(tx, &d.supersedes, KIND_DECISION, &[], project)?;
                add_edge(
                    tx,
                    &d.id,
                    &d.supersedes,
                    REL_SUPERSEDES,
                    at,
                    e.position,
                    project,
                    TIER_EXTRACTED,
                )?;
                // Invalidate (never delete) the governing edges the superseded decision
                // asserted - scoped to this project's edges, so a shared-backend fold never
                // touches another project's edge that happens to share the from_id.
                tx.execute(
                    "UPDATE edges SET valid_to = ?1
                     WHERE from_id = ?2 AND rel = ?3 AND valid_to IS NULL AND project = ?4",
                    params![at, d.supersedes, REL_GOVERNS, project],
                )
                .map_err(be)?;
            }
        }
        TYPE_FILE_TOUCHED => {
            // De-noise (spec 43): `agent --TOUCHES--> file` is run machinery, not the target
            // project's structure, so this arm is a graph no-op. The FileTouched event stays in
            // the log (metrics and the run-tree read it); it just no longer projects a KIND_AGENT
            // node or a REL_TOUCHES edge. The file's node, when it is one, is created by the code
            // structure folds and by the decisions/findings that GOVERN / are ABOUT it.
        }
        TYPE_GATE_VERDICT => {
            // De-noise (spec 43): a gate is run machinery, not the target project, so this arm is
            // a graph no-op - no KIND_GATE node, no REL_GATED_BY edge. The GateVerdict event stays
            // in the log, where metrics (per-gate remediation counts) and the run-tree read it.
        }
        TYPE_UNIT_STARTED => {
            // De-noise (spec 43): a unit is run machinery, not the target project, so this arm is
            // a graph no-op - no KIND_UNIT node, no KIND_AGENT node, no REL_ASSIGNED_TO / REL_BLOCKS
            // edge. The UnitStarted event stays in the log; the run-tree projects units/stages
            // straight from events (its proper home), and metrics reads it for units-started.
        }
        TYPE_UNIT_INTEGRATED => {
            let u: super::UnitIntegrated = serde_json::from_slice(&e.data).map_err(be)?;
            // De-noise (spec 43): no KIND_UNIT node is projected (a unit is run machinery). But the
            // integrate still drives disposition-expiry below - the LIFECYCLE the fold owns, which
            // reads the finding's `$.unit` string attribute (a token, never a KIND_UNIT node) and
            // is therefore unaffected by dropping the unit node.
            // Disposition-expiry (spec 25, criterion 2 - the UPHELD-AND-ADDRESSED trigger's
            // INVALIDATE half): integrating a unit ADDRESSES every finding its review upheld,
            // so those findings are now resolved. The adjudicator's earlier SpawnResult marked
            // each upheld finding-of-this-unit (disposition=upheld, unit=<this unit>); expire
            // them now through the same shared authority the discard trigger uses. A finding
            // upheld for a DIFFERENT unit, or upheld here but re-raised under a later run, carries
            // no matching mark and is untouched - keeping the invalidation run-scoped by
            // construction. Collect the marked ids deterministically (ORDER BY id) before
            // mutating so the fold order never varies.
            //
            // Corrected (spec 86 criterion 2 round 3, arch-u86c2-r2-ensure-node-merge-silently-
            // narrows-spec25-disposition-mark-erasure): a re-raise no longer clears the mark via a
            // whole-blob COALESCE-replace - `ensure_node`'s `ON CONFLICT` now `json_patch`-MERGES
            // (spec 86 criterion 2 round 2, so evidence attrs survive an unrelated structural
            // re-fold). The run-scoping guarantee above ("upheld here but re-raised under a later
            // run... carries no matching mark") still holds, but for a DIFFERENT, more fragile
            // reason: `TYPE_REVIEW_FINDING`'s own `ensure_node` call (below) unconditionally
            // re-supplies `unit` (production's `ReviewFinding.unit` defaults to an empty string) on
            // EVERY fold including a re-raise, and json_patch OVERWRITES a key the incoming attrs
            // DO mention - so the re-raise still knocks this SELECT's `unit = ?2` out of matching.
            // `disposition` itself, which that same call never mentions, is NOT cleared any more
            // (json_patch preserves a key the incoming attrs leave unmentioned) and persists on the
            // node forever after a single upheld mark - harmless today only because nothing else
            // ever reads `$.disposition` (grepped), so a re-raised finding that later integrates
            // still correctly stays LIVE (the `unit` mismatch alone already prevents this SELECT
            // from matching it). If a future change ever narrows `TYPE_REVIEW_FINDING`'s own
            // `ensure_node` attrs to drop the near-always-empty `unit` key, this protection
            // disappears silently with every gate still green - see
            // `an_upheld_mark_never_expires_the_same_finding_re_raised_before_its_unit_integrates`'s
            // own updated doc below for the regression this guards.
            let marked: Vec<String> = {
                let mut stmt = tx
                    .prepare(
                        "SELECT id FROM nodes
                          WHERE kind = ?1
                            AND json_extract(attrs, '$.disposition') = 'upheld'
                            AND json_extract(attrs, '$.unit') = ?2
                            AND project = ?3
                          ORDER BY id",
                    )
                    .map_err(be)?;
                let ids = stmt
                    .query_map(params![KIND_FINDING, u.unit, project], |r| {
                        r.get::<_, String>(0)
                    })
                    .map_err(be)?
                    .collect::<Result<_, _>>()
                    .map_err(be)?;
                ids
            };
            for fid in &marked {
                invalidate_finding_edges(tx, fid, at, project)?;
            }
        }
        TYPE_LESSON_LEARNED => {
            let l: super::LessonLearned = serde_json::from_slice(&e.data).map_err(be)?;
            ensure_node(tx, &l.id, KIND_LESSON, &[("summary", &l.summary)], project)?;
            for path in super::locations(&l.about, &l.file) {
                let canonical = resolve_in_tx(tx, path);
                ensure_node(tx, &canonical, KIND_ARTIFACT, &[], project)?;
                add_edge(
                    tx,
                    &l.id,
                    &canonical,
                    REL_ABOUT,
                    at,
                    e.position,
                    project,
                    TIER_EXTRACTED,
                )?;
            }
        }
        TYPE_REVIEW_FINDING => {
            // A review finding the lenses / adversary raise about a unit's files:
            // the cross-agent memory the three tiers communicate THROUGH. The finding
            // node carries the summary, the reviewer (`by`), and the unit; an ABOUT
            // edge ties it to each file it concerns, its `about` list and its `file`
            // (so a later reviewer grounded on those files reaches it the same way it
            // reaches the decisions that GOVERN them). De-noise (spec 43): the reviewer's provenance is NOT projected as a
            // KIND_AGENT node or a REL_RAISED edge - that agent attribution is run
            // machinery, not the target project. The `by` reviewer stays as a node
            // ATTRIBUTE on the finding (read by consumers off the content node), and the
            // event's actor stays in the log for metrics and the run-tree.
            let f: super::ReviewFinding = serde_json::from_slice(&e.data).map_err(be)?;
            ensure_node(
                tx,
                &f.id,
                KIND_FINDING,
                &[("summary", &f.summary), ("by", &f.by), ("unit", &f.unit)],
                project,
            )?;
            for path in super::locations(&f.about, &f.file) {
                let canonical = resolve_in_tx(tx, path);
                ensure_node(tx, &canonical, KIND_ARTIFACT, &[], project)?;
                add_edge(
                    tx,
                    &f.id,
                    &canonical,
                    REL_ABOUT,
                    at,
                    e.position,
                    project,
                    TIER_EXTRACTED,
                )?;
            }
        }
        TYPE_SPAWN_RESULT => {
            // Disposition-expiry (spec 25, criterion 1 - the DISCARD trigger): an
            // adjudicator's recorded result is where a review's findings are RESOLVED. A
            // finding the adjudicator NAMES in its verdict line's `discarded` array is
            // DISCARDED, so invalidate (set valid_to, never delete - mirroring the
            // decision-supersession arm above) its RAISED / ABOUT edges; the live `subgraph`
            // filter (valid_to IS NULL) then prunes it so agents ground on LIVE findings only.
            //
            // Keying on the EXPLICIT `discarded` finding ids - a field production sets on
            // every real finding (`data.id`, stamped back into the verdict by the
            // adjudicator) - is deliberate on two counts. (1) It fires against what
            // production records: a real finding carries NO `$.unit` attr (cmd_emit / the MCP
            // server stamp only `meta.spawn`, and `ReviewFinding.unit` defaults empty), so a
            // fold keyed on `json_extract(attrs,'$.unit')` would match nothing and expire
            // nothing. (2) It never over-invalidates: the discard is NOT the complement of
            // `upheld` (56/234 adjudications approve with no `upheld` at all), so a verdict
            // that omits `upheld` never sweeps a review's still-open findings, and a reject's
            // own motivating findings stay live for the remediation unless the adjudicator
            // explicitly discarded them. The disposition is read through the single
            // `SpawnResult::adjudication` authority the review-quality metric also reads (it
            // self-gates on the adjudicator role, so a non-adjudicator result yields nothing),
            // keeping the graph and the metric on one story. An unparseable result
            // graceful-skips (mirroring the metrics fold), so one malformed event never wedges
            // a whole rebuild.
            if let Ok(res) = SpawnResult::from_event(e) {
                if let Some(adj) = res.adjudication() {
                    // Determinism (no HashMap iteration): the discarded ids through a BTreeSet,
                    // so the invalidations run in a fixed id order whatever order the verdict
                    // array listed them.
                    let discarded: BTreeSet<String> = adj.discarded.into_iter().collect();
                    for fid in &discarded {
                        // Invalidate the discarded finding's provenance (RAISED) and file
                        // (ABOUT) edges through the shared single authority below.
                        invalidate_finding_edges(tx, fid, at, project)?;
                    }
                    // Disposition-expiry (spec 25, criterion 2 - the UPHELD-AND-ADDRESSED
                    // trigger's MARK half): a finding the adjudicator UPHELD is not yet
                    // resolved - it is resolved only when the unit that owns it INTEGRATES and
                    // addresses it. So MARK each upheld finding with its disposition and the
                    // unit it belongs to (the adjudicator spawn id's unit token, the same split
                    // metrics.rs reads), and let the TYPE_UNIT_INTEGRATED arm invalidate it on
                    // integration. json_set MERGES the two keys into the finding's existing
                    // attrs so its summary / by survive - an upheld-but-not-yet-integrated
                    // finding still renders live in grounding until its unit lands. The guard
                    // `kind = KIND_FINDING` keeps a stray upheld id from stamping another node.
                    let unit = res.id.split('/').next().unwrap_or(&res.id);
                    let upheld: BTreeSet<String> = adj.upheld.into_iter().collect();
                    for fid in &upheld {
                        tx.execute(
                            "UPDATE nodes
                                SET attrs = json_set(
                                    COALESCE(attrs, '{}'), '$.disposition', 'upheld', '$.unit', ?2)
                              WHERE id = ?1 AND kind = ?3 AND project = ?4",
                            params![fid, unit, KIND_FINDING, project],
                        )
                        .map_err(be)?;
                    }
                }
            }
        }
        TYPE_ALIAS_DEFINED => {
            let a: super::AliasDefined = serde_json::from_slice(&e.data).map_err(be)?;
            tx.execute(
                "INSERT INTO aliases (alias, canonical_id) VALUES (?1, ?2)
                 ON CONFLICT(alias) DO UPDATE SET canonical_id = excluded.canonical_id",
                params![a.alias, a.canonical],
            )
            .map_err(be)?;
        }
        TYPE_ALIAS_UNRESOLVED => {
            let a: super::AliasUnresolved = serde_json::from_slice(&e.data).map_err(be)?;
            // Create the node and mark it unresolved for later merge (never drop).
            ensure_node(
                tx,
                &a.mention,
                KIND_ARTIFACT,
                &[("unresolved", "true")],
                project,
            )?;
        }
        _ => {}
    }
    Ok(())
}

/// Invalidate (set `valid_to`, never delete - mirroring the decision-supersession arm) the
/// RAISED (provenance, into it) and ABOUT (file, out of it) edges of a RESOLVED finding, so the
/// live `subgraph` filter (`valid_to IS NULL`) stops returning it and agents ground on LIVE
/// findings only. Guarded by `EXISTS(KIND_FINDING)` so a stray id can never expire another node
/// kind's edges. This is the single edge-invalidation authority both disposition-expiry triggers
/// (spec 25) share: the DISCARD trigger on an adjudicator's result, and the UPHELD-AND-ADDRESSED
/// trigger on a unit's integration. Only edges that currently hold (`valid_to IS NULL`) are
/// touched, so an already-invalidated edge and a later run's fresh re-raise are both left alone.
fn invalidate_finding_edges(
    tx: &Transaction,
    fid: &str,
    at: i64,
    project: &str,
) -> Result<(), Error> {
    tx.execute(
        "UPDATE edges SET valid_to = ?1
         WHERE valid_to IS NULL
           AND project = ?6
           AND ((from_id = ?2 AND rel = ?3) OR (to_id = ?2 AND rel = ?4))
           AND EXISTS (SELECT 1 FROM nodes WHERE id = ?2 AND kind = ?5 AND project = ?6)",
        params![at, fid, REL_ABOUT, REL_RAISED, KIND_FINDING, project],
    )
    .map_err(be)?;
    Ok(())
}

/// Collapse a mention onto its canonical node via the alias table; an unknown
/// mention resolves to itself.
fn resolve_in_tx(tx: &Transaction, mention: &str) -> String {
    tx.query_row(
        "SELECT canonical_id FROM aliases WHERE alias = ?1",
        [mention],
        |r| r.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
    .unwrap_or_else(|| mention.to_string())
}

/// Fetch a node's `(id, kind, attrs)` scoped to `project`, or `None` when it does not exist there
/// (spec 52). Read isolation matches [`ContextGraph::subgraph`]: a same-id row in another project is
/// never returned. Used to seed the directed walk on real nodes only and to materialize each
/// reached node's kind/attrs for the result.
fn node_row(conn: &Connection, id: &str, project: &str) -> Result<Option<Node>, Error> {
    conn.query_row(
        "SELECT id, kind, attrs FROM nodes WHERE id = ?1 AND project = ?2",
        params![id, project],
        row_to_node,
    )
    .optional()
    .map_err(be)
}

/// The store form of [`node_leaf`](super::query::node_leaf) over a node row's `attrs` (spec 119):
/// its non-empty `name`, NULL for an absent or empty one, where `node_leaf` reads the id. ONE pinned
/// expression the leaf index (`idx_nodes_leaf`, [`migrate_indexes`]) is built on and every
/// node-side reader embeds verbatim, since SQLite serves an expression index only to the identical
/// expression; `IS NOT NULL` over it is the store's one name-presence test.
fn node_leaf_sql() -> &'static str {
    "NULLIF(json_extract(attrs, '$.name'), '')"
}

/// The `<file>::` id range (spec 119): `column` holds an id whose file half is the SQL value
/// `file`, an id `<file>::<rest>` sorting at or after `<file>::` and before `<file>:;` (`;` the
/// byte after `:`), so a seek of an index on `column` answers it and no `LIKE`/`GLOB` wildcard a
/// real path could hold is read: the spelling a file's members ([`Neighbors::members`]) embed.
fn id_range_sql(column: &str, file: &str) -> String {
    format!("{column} >= {file} || '::' AND {column} < {file} || ':;'")
}

/// The store form of [`member_file`](super::query::member_file) over a node row's `kind`, `id`
/// and `attrs` (spec 119, THE MEMBER RULE): the file half of a `code-entity` row's id when its
/// [`node_leaf_sql`] is not NULL, else NULL, and NULL too for an id holding no `::` or an empty
/// file half, where `query::node_file` names none. The expression a file's members
/// ([`Neighbors::members`]) embed, so a placeholder, a bare parent, a bare caller or a
/// knowledge-held flat id enters a reach only through an edge.
fn member_sql() -> String {
    let leaf = node_leaf_sql();
    format!(
        "CASE WHEN kind = '{KIND_CODE_ENTITY}' AND {leaf} IS NOT NULL
              THEN {} END",
        file_half_sql("id")
    )
}

/// The text of an edge's target after its first `::`, `to_id` naming the target column: ONE pinned
/// expression the partial index `idx_edges_live_to_name_suffix` ([`migrate_indexes`]) is built on
/// over `to_id` and every edge-side reader embeds through [`edge_binds_leaf_sql`], since SQLite
/// serves an expression index only to the identical expression.
fn edge_text_sql(to_id: &str) -> String {
    format!("substr({to_id}, instr({to_id}, '::') + 2)")
}

/// THE EDGE SIDE BINDS THE LEAF (spec 119): the live edge's target text ([`edge_text_sql`], the
/// expression the partial index `idx_edges_live_to_name_suffix` is built on), compared with the
/// SQL value `leaf` and that value prefixed by [`PLACEHOLDER_SIGIL`]: two seeks of the index, the
/// first matching a target spelled `<file>::<leaf>`, the second one spelled `<file>::%<leaf>`, so
/// the binding holds under both spellings and decides nothing about where a reference lands.
/// `to_id` names the edge's target column. The spelling a leaf's nodes ([`Neighbors::leaf_nodes`])
/// embed.
fn edge_binds_leaf_sql(to_id: &str, leaf: &str) -> String {
    let text = edge_text_sql(to_id);
    format!("{text} IN ({leaf}, '{PLACEHOLDER_SIGIL}' || {leaf})")
}

/// The statement `verb` over the `edges` rows the history prune reclaims (spec 41), past the
/// further conditions `exclusions`, so [`Projector::count_prunable`] counts and [`prune_in`]
/// deletes one set: an edge retired before the boundary `?1` - a live edge's NULL `valid_to` is
/// before no boundary - in project `?2`.
fn superseded_edges_sql(verb: &str, exclusions: &str) -> String {
    format!("{verb} FROM edges WHERE valid_to < ?1 AND project = ?2 {exclusions}")
}

/// The file half of the id `column` holds, cut in SQL at its first `::`: NULL for an id with no
/// `::` or an empty file half, where [`split_id`] names the whole id or an empty file.
fn file_half_sql(column: &str) -> String {
    format!("NULLIF(substr({column}, 1, instr({column}, '::') - 1), '')")
}

fn ensure_node(
    tx: &Transaction,
    id: &str,
    kind: &str,
    attrs: &[(&str, &str)],
    project: &str,
) -> Result<(), Error> {
    let attr_json: Option<String> = if attrs.is_empty() {
        None
    } else {
        let map: BTreeMap<&str, &str> = attrs.iter().copied().collect();
        Some(serde_json::to_string(&map).map_err(be)?)
    };
    // The project scope (spec 28) is part of the node's identity: the conflict target is
    // (id, project), so the SAME id under a DIFFERENT project is a distinct row, never an
    // upsert over another project's node.
    //
    // One kind per (id, project): a more specific kind promotes a bare KIND_ARTIFACT and a later
    // KIND_ARTIFACT never demotes it ([`settled_kind`]), so the kind is a function of the source,
    // not of log interleaving. This is the single node-mutation authority, so the reconciliation
    // lives here rather than in a second UPDATE path.
    //
    // MERGE, never whole-blob replace: `json_patch` (RFC 7396) keeps every stored key the incoming
    // attrs do not mention. A second writer shares a node with this one - the spec-25
    // disposition-expiry mark (`$.disposition`/`$.unit`, a raw `json_set` on a KIND_FINDING node
    // `TYPE_REVIEW_FINDING` also writes) - which a re-raise therefore keeps; the
    // `TYPE_UNIT_INTEGRATED` arm's doc works out why run scoping still holds. Empty incoming attrs
    // (`excluded.attrs IS NULL`) leave the stored ones untouched.
    tx.execute(
        "INSERT INTO nodes (id, kind, attrs, project) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(id, project) DO UPDATE SET
             attrs = CASE
                 WHEN excluded.attrs IS NULL THEN nodes.attrs
                 ELSE json_patch(COALESCE(nodes.attrs, '{}'), excluded.attrs)
             END,
             kind = CASE
                 WHEN nodes.kind = ?5 AND excluded.kind IN (?6, ?7, ?8, ?9, ?10, ?11)
                     THEN excluded.kind
                 ELSE nodes.kind
             END",
        params![
            id,
            kind,
            attr_json,
            project,
            KIND_ARTIFACT,
            PROMOTING_KINDS[0],
            PROMOTING_KINDS[1],
            PROMOTING_KINDS[2],
            PROMOTING_KINDS[3],
            PROMOTING_KINDS[4],
            PROMOTING_KINDS[5],
        ],
    )
    .map_err(be)?;
    record_assertion(tx, id, kind, attr_json.as_deref(), project)
}

/// The specific kinds that PROMOTE a bare [`KIND_ARTIFACT`] path node ([`ensure_node`]): each
/// proves what the path is - a source file, a code entity in one (spec 101: a decision may name
/// `<file>::<path>` before or after the definition folds), or a design-intent document.
const PROMOTING_KINDS: [&str; 6] = [
    KIND_FILE,
    KIND_CODE_ENTITY,
    KIND_DESIGN_DOC,
    KIND_ARCH_DECISION,
    KIND_HANDBOOK_RULE,
    KIND_RATIONALE,
];

/// The kind a node takes from assertions of `kinds`, in order: the first one, promoted from
/// [`KIND_ARTIFACT`] by the first [`PROMOTING_KINDS`] member after it - the rule [`ensure_node`]'s
/// upsert applies one assertion at a time.
fn settled_kind<'k>(kinds: impl IntoIterator<Item = &'k str>) -> Option<&'k str> {
    kinds.into_iter().fold(None, |settled, k| match settled {
        Some(KIND_ARTIFACT) if PROMOTING_KINDS.contains(&k) => Some(k),
        None => Some(k),
        kept => kept,
    })
}

/// Record the log's assertion of the node `id` as `kind` with `attrs` (identity-empty: no later
/// generation retracts it), merging `attrs` into the one it recorded before and keeping the kind
/// it settled ([`settled_kind`]).
fn record_assertion(
    tx: &Transaction,
    id: &str,
    kind: &str,
    attrs: Option<&str>,
    project: &str,
) -> Result<(), Error> {
    let recorded: Option<String> = tx
        .query_row(
            "SELECT kind FROM node_assertions
              WHERE project = ?1 AND identity = '' AND node_id = ?2",
            params![project, id],
            |r| r.get(0),
        )
        .optional()
        .map_err(be)?;
    let settled = match &recorded {
        Some(recorded) => settled_kind([recorded.as_str(), kind]).unwrap_or(kind),
        None => kind,
    };
    tx.execute(
        "INSERT INTO node_assertions (project, identity, generation, node_id, kind, attrs)
         VALUES (?1, '', '', ?2, ?3, ?4)
         ON CONFLICT (project, identity, node_id) DO UPDATE SET
             kind = excluded.kind,
             attrs = CASE
                 WHEN excluded.attrs IS NULL THEN node_assertions.attrs
                 ELSE json_patch(COALESCE(node_assertions.attrs, '{}'), excluded.attrs)
             END",
        params![project, id, settled, attrs],
    )
    .map(drop)
    .map_err(be)
}

/// Assert a live edge, UPSERT-LIVE (spec 40): at most one live edge per
/// `(from_id, to_id, rel, tier, project)`. Every fold arm re-asserts relationships over time - a
/// `FileTouched` refolds `agent --TOUCHES--> file` on EVERY touch, a re-run refolds `GOVERNS` /
/// `ABOUT`, and so on. A bare `INSERT ... valid_to = NULL` therefore accreted an identical live
/// row per fold, bloating the graph and the grounding slice injected into every prompt. So before
/// inserting, look for the existing LIVE edge with this exact key: if one is present, record the
/// latest assertion in place - bump `source` to the newest position, keep the EARLIEST `valid_from`
/// (the fact has held since it first became true) - and add NO row; otherwise INSERT as before.
///
/// Keyed on LIVE edges only (`valid_to IS NULL`), so it never suppresses a legitimate re-assertion
/// AFTER an invalidation: a superseded `GOVERNS` (its `valid_to` set) that is later re-asserted
/// correctly folds a NEW live edge. Dedup collapses only EXACT duplicates (identical
/// `from`/`to`/`rel`/`tier`), so it never merges two DISTINCT edges - the safe superset is
/// preserved. `max`/`min` are the scalar SQLite functions, making the update order-independent so a
/// rebuild from the log re-derives byte-identical provenance regardless of fold order. This one
/// localized change dedups every fold arm at once, without touching a single call site.
#[allow(clippy::too_many_arguments)]
fn add_edge(
    tx: &Transaction,
    from: &str,
    to: &str,
    rel: &str,
    at: i64,
    src: Position,
    project: &str,
    tier: &str,
) -> Result<i64, Error> {
    let live: Option<i64> = tx
        .query_row(
            "UPDATE edges SET source = max(source, ?5), valid_from = min(valid_from, ?4)
             WHERE from_id = ?1 AND to_id = ?2 AND rel = ?3 AND tier = ?7 AND project = ?6
               AND valid_to IS NULL
             RETURNING id",
            params![from, to, rel, at, src as i64, project, tier],
            |r| r.get(0),
        )
        .optional()
        .map_err(be)?;
    if let Some(id) = live {
        return Ok(id);
    }
    tx.query_row(
        "INSERT INTO edges (from_id, to_id, rel, valid_from, valid_to, source, project, tier)
         VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7)
         RETURNING id",
        params![from, to, rel, at, src as i64, project, tier],
        |r| r.get(0),
    )
    .map_err(be)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;
    // Machinery kinds/rels the fold no longer projects (spec 43 de-noise): imported here because
    // only these tests - which PROVE the machinery is gone - still name them. REL_RAISED stays in
    // the module-level import (invalidate_finding_edges still references it).
    use super::super::{
        KIND_AGENT, KIND_GATE, KIND_UNIT, META_ACTOR, REL_ASSIGNED_TO, REL_BLOCKS, REL_CALLS,
        REL_DECIDED, REL_GATED_BY, REL_REFERENCES, REL_TOUCHES, TIER_AMBIGUOUS, TIER_INFERRED,
    };
    use crate::test_support::{
        apply_decision, apply_governs_at, apply_json, apply_json_as, decision_json, dir_snapshot,
        edge_tier,
    };
    use crate::test_support::{records_graph, RecordsRead};
    use rigger_domain::contextgraph::{Direction, Located};

    #[test]
    fn prune_drops_the_named_nodes_and_their_edges_and_a_replay_does_not_resurrect_them() {
        // Spec 21, unit 2: the single graph-mutation authority `rigger reset --runs` uses. It
        // deletes the named decision/finding nodes plus EVERY edge touching them, leaves the
        // rest (including any lesson, which the caller never puts in the drop set, and the
        // shared file itself), and - because the `applied` position ledger is untouched - a
        // replay of a pruned node's own event never resurrects it (drop from the graph WITHOUT
        // wiping the store).
        let p = Projector::open(":memory:", "test").unwrap();
        apply_decision(&p, 1, "keep-d", "keep", &["shared.rs"], "");
        apply_decision(&p, 2, "drop-d", "drop", &["shared.rs"], "");
        // A finding to drop, about the same file, raised by a reviewer (so it has a RAISED
        // edge INTO it and an ABOUT edge OUT of it - both must be swept).
        let finding = serde_json::json!({
            "id": "drop-f", "by": "lens", "summary": "x", "about": ["shared.rs"],
        });
        let mut fe = Event::new(TYPE_REVIEW_FINDING, serde_json::to_vec(&finding).unwrap());
        fe.position = 3;
        crate::test_support::folds(&p, std::slice::from_ref(&fe));
        // A lesson about the same file: the caller never drops a lesson, so it must survive.
        let lesson =
            serde_json::json!({"id": "keep-lesson", "summary": "y", "about": ["shared.rs"]});
        let mut le = Event::new(TYPE_LESSON_LEARNED, serde_json::to_vec(&lesson).unwrap());
        le.position = 4;
        crate::test_support::folds(&p, std::slice::from_ref(&le));

        // Before: every node is reachable from the shared file.
        let before = p.subgraph(&["shared.rs".to_string()], 2).unwrap();
        for id in ["keep-d", "drop-d", "drop-f", "keep-lesson"] {
            assert!(
                before.nodes.iter().any(|n| n.id == id),
                "{id} present before prune"
            );
        }

        let removed = p
            .prune(&["drop-d".to_string(), "drop-f".to_string()], None)
            .unwrap();
        assert_eq!(removed.nodes, 2, "exactly the two named nodes are removed");
        assert_eq!(
            removed.superseded_edges, 0,
            "a node-only prune (None boundary) reclaims no superseded edges"
        );

        let after = p.subgraph(&["shared.rs".to_string()], 2).unwrap();
        for id in ["drop-d", "drop-f"] {
            assert!(
                !after.nodes.iter().any(|n| n.id == id),
                "{id} is pruned from the graph"
            );
            assert!(
                !after.edges.iter().any(|e| e.from == id || e.to == id),
                "every edge touching {id} is pruned"
            );
        }
        for id in ["keep-d", "keep-lesson", "shared.rs"] {
            assert!(
                after.nodes.iter().any(|n| n.id == id),
                "{id} is preserved (only the named nodes are pruned)"
            );
        }

        // A replay of the pruned decision's event (same position) does NOT resurrect it: the
        // position is still marked applied, so the fold is a no-op - the prune is durable.
        apply_decision(&p, 2, "drop-d", "drop", &["shared.rs"], "");
        let replayed = p.subgraph(&["shared.rs".to_string()], 2).unwrap();
        assert!(
            !replayed.nodes.iter().any(|n| n.id == "drop-d"),
            "a pruned node is not resurrected by a replay of its event"
        );
    }

    #[test]
    fn subgraph_finds_the_governing_decision() {
        let p = Projector::open(":memory:", "test").unwrap();
        apply_decision(&p, 1, "d1", "uses the generic pipeline", &["mod.rs"], "");
        let g = p.subgraph(&["mod.rs".to_string()], 2).unwrap();
        let d = g
            .nodes
            .iter()
            .find(|n| n.id == "d1")
            .expect("d1 reachable from mod.rs");
        assert_eq!(
            d.attrs.get("summary").map(String::as_str),
            Some("uses the generic pipeline")
        );
    }

    #[test]
    fn supersession_invalidates_the_old_governing_edge() {
        let p = Projector::open(":memory:", "test").unwrap();
        apply_decision(&p, 1, "d1", "old", &["mod.rs"], "");
        apply_decision(&p, 2, "d2", "new", &["mod.rs"], "d1");
        let g = p.subgraph(&["mod.rs".to_string()], 2).unwrap();
        let governs: Vec<(&str, &str)> = g
            .edges
            .iter()
            .filter(|e| e.rel == REL_GOVERNS)
            .map(|e| (e.from.as_str(), e.to.as_str()))
            .collect();
        assert!(
            governs.contains(&("d2", "mod.rs")),
            "d2 currently governs mod.rs"
        );
        assert!(
            !governs.contains(&("d1", "mod.rs")),
            "d1's GOVERNS edge was invalidated"
        );
    }

    #[test]
    fn apply_is_idempotent_per_position() {
        let p = Projector::open(":memory:", "test").unwrap();
        apply_decision(&p, 1, "d1", "x", &["mod.rs"], "");
        apply_decision(&p, 1, "d1", "x", &["mod.rs"], ""); // same position, replayed
        let g = p.subgraph(&["mod.rs".to_string()], 2).unwrap();
        let governs = g.edges.iter().filter(|e| e.rel == REL_GOVERNS).count();
        assert_eq!(governs, 1, "a replayed event must not double the edge");
    }

    /// spec 49 criterion 2 (graph side): `apply_batch` folds a WHOLE batch in ONE call to the SAME
    /// graph that folding each event with `apply` would produce, and stays idempotent per position -
    /// so the batched-fold cadence changes only the transaction count, never the graph rows.
    #[test]
    fn apply_batch_folds_a_batch_equivalently_to_per_event_applies_and_is_idempotent() {
        let decision = |pos: u64, id: &str, path: &str| -> Event {
            let payload = serde_json::json!({ "id": id, "summary": "x", "governs": [path], "supersedes": "" });
            let mut e = Event::new(TYPE_DECISION_MADE, serde_json::to_vec(&payload).unwrap());
            e.position = pos;
            e
        };
        let batch = vec![
            decision(1, "d1", "a.rs"),
            decision(2, "d2", "b.rs"),
            decision(3, "d3", "c.rs"),
        ];

        // Reference: fold each event one at a time.
        let per_event = Projector::open(":memory:", "test").unwrap();
        crate::test_support::folds_each(&per_event, &batch);

        // Batched: fold the whole slice in ONE call.
        let batched = Projector::open(":memory:", "test").unwrap();
        crate::test_support::folds(&batched, &batch);

        assert_eq!(
            live_governs(&batched).len(),
            3,
            "apply_batch folds all three decisions' GOVERNS edges in one call"
        );
        assert_eq!(
            live_governs(&batched),
            live_governs(&per_event),
            "apply_batch yields the SAME graph as folding each event with apply"
        );

        // Idempotent per position: re-applying the SAME batch at the same positions adds nothing.
        crate::test_support::folds(&batched, &batch);
        assert_eq!(
            live_governs(&batched).len(),
            3,
            "re-applying a batch at the same positions must not double any edge"
        );
    }

    /// spec 49 criterion 2 (graph side): `apply_batch` folding directly, on a projector handed no
    /// log, is ONE transaction - a fold error partway through rolls the WHOLE batch back, so it is
    /// never half-applied. This distinguishes the
    /// single-transaction override from a per-event loop (which would have committed the earlier
    /// events before the later one failed), pinning the batched cadence itself, not just its result.
    #[test]
    fn apply_batch_is_atomic_a_mid_batch_fold_error_rolls_the_whole_batch_back() {
        // A good decision, then a POISON event: a DecisionMade whose data is not valid JSON, so its
        // fold errors AFTER the good event has already folded WITHIN the same transaction.
        let good = {
            let payload = serde_json::json!({
                "id": "d1", "summary": "x", "governs": ["a.rs"], "supersedes": ""
            });
            let mut e = Event::new(TYPE_DECISION_MADE, serde_json::to_vec(&payload).unwrap());
            e.position = 1;
            e
        };
        let poison = {
            let mut e = Event::new(TYPE_DECISION_MADE, crate::test_support::POISON.to_vec());
            e.position = 2;
            e
        };

        let p = Projector::open(":memory:", "test").unwrap();
        assert!(
            matches!(
                crate::test_support::fold_of(&p, &[good.clone(), poison]),
                crate::contextgraph::Fold::NotFolded(_)
            ),
            "a fold error must surface from apply_batch"
        );
        assert_eq!(
            live_governs(&p).len(),
            0,
            "the good event folded before the poison must be ROLLED BACK with the failed batch"
        );

        // The `applied` guard was rolled back too, so a retry re-folds the good event cleanly.
        crate::test_support::folds(&p, std::slice::from_ref(&good));
        assert_eq!(
            live_governs(&p).len(),
            1,
            "after the rollback the good event still folds on its own (its position was not consumed)"
        );
    }

    /// A batch that fails to fold into a current `graph.db` writes no mark (spec 124, THE
    /// APPENDER): the same projector and a fresh open owe nothing and the next fold is not
    /// refused, while the ledger misses the lost position below the one folded after it - so once
    /// the hole is older than the catch-up horizon, the seeding `rigger setup` runs finds the
    /// prefix violated and a rebuild from the log pays it, folding what was lost, and is then not
    /// owed again.
    #[test]
    fn a_failed_fold_into_a_current_file_writes_no_mark_and_a_rebuild_pays_its_old_hole() {
        use crate::contextgraph::Fold;
        let decision = |id: &str, file: &str, position: u64| {
            let payload = serde_json::json!({
                "id": id, "summary": "x", "governs": [file], "supersedes": ""
            });
            let mut e = Event::new(TYPE_DECISION_MADE, serde_json::to_vec(&payload).unwrap());
            e.position = position;
            e
        };
        let mut poison = Event::new(TYPE_DECISION_MADE, crate::test_support::POISON.to_vec());
        poison.position = 2;
        let log = crate::test_support::backdated(&[
            decision("d1", "a.rs", 1),
            poison,
            decision("d3", "b.rs", 3),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();
        let mark = crate::test_support::owed_mark_in(dir.path());

        let p = Projector::open(path, "test").unwrap();
        crate::test_support::folds(&p, &log[..1]);
        assert_eq!(
            (p.rebuild_owed().unwrap(), mark.exists()),
            (false, false),
            "a fold that succeeds owes nothing"
        );
        assert!(matches!(
            crate::test_support::fold_of(&p, &log[1..2]),
            Fold::NotFolded(_)
        ));
        assert_eq!(
            (
                p.rebuild_owed().unwrap(),
                mark.exists(),
                crate::test_support::fold_of(&p, &log[2..]),
                Projector::open(path, "test")
                    .unwrap()
                    .rebuild_owed()
                    .unwrap(),
            ),
            (false, false, Fold::Folded, false),
            "the failed fold writes no mark: neither the same projector nor a fresh open owes, and \
             the next fold is not refused"
        );
        drop(p);

        let rebuilt = rebuild_as_setup_does(path, &log);
        let p = Projector::open(path, "test").unwrap();
        assert_eq!(
            (rebuilt, mark.exists(), p.rebuild_owed().unwrap()),
            (true, false, false),
            "the rebuild pays the ledger's old hole, writing no mark"
        );
        let governed: Vec<String> = live_governs(&p).into_iter().map(|g| g.1).collect();
        assert_eq!(
            governed,
            vec!["a.rs".to_string(), "b.rs".to_string()],
            "the rebuilt file holds the event the refused fold lost"
        );
        assert_eq!(
            (applied_through(&p, 3), crate::test_support::caught_up_at(Path::new(path))),
            (vec![true; 3], Some(3)),
            "the rebuild records the event it passed over, so the ledger holds every position, and \
             seeds the cursor at its last"
        );
        drop(p);
        let again = Projector::open(path, "test")
            .unwrap()
            .rebuild(
                &locked(path),
                false,
                &mut |_, _| panic!("a paid rebuild reads nothing"),
                &mut |_| {},
            )
            .unwrap();
        assert_eq!(again, None, "a paid rebuild does not run again");
    }

    /// The `RunStarted` of run `run` at `position`.
    fn run_started_at(run: &str, position: u64) -> Event {
        event_at(
            rigger_domain::run::TYPE_RUN_STARTED,
            format!(r#"{{"run":"{run}","spec":"s.md"}}"#).as_bytes(),
            position,
        )
    }

    /// An event of `type_` at `position` whose payload is `data`.
    fn event_at(type_: &str, data: &[u8], position: u64) -> Event {
        let mut e = Event::new(type_, data.to_vec());
        e.position = position;
        e
    }

    /// A lesson at `position` whose payload, an empty array, the judge rejects.
    fn rejected_lesson_at(position: u64) -> Event {
        event_at(TYPE_LESSON_LEARNED, b"[]", position)
    }

    /// What the rebuild source of the tests that stop a rebuild in its tail answers there.
    const TAIL_INTERRUPTED: &str = "interrupted in its tail";

    /// What the rebuild source of the tests that stop a rebuild past its first batch answers there.
    const FIRST_BATCH_INTERRUPTED: &str = "interrupted after the first batch";

    /// The rebuild lock of the graph file at `path`, which no other holder has.
    fn locked(path: &str) -> RebuildLock {
        Projector::lock_rebuild(path).unwrap()
    }

    /// A connection of its own to the graph file at `path`, through the opener every connection to
    /// that file is opened through.
    fn connection_to(path: &str) -> Connection {
        open_connection(path).unwrap()
    }

    /// A rebuild of the graph file at `path` from `log` in batches of `batch`, owed by its caller,
    /// once it takes the rebuild lock, recording where each read of the log started.
    fn rebuild_in_batches(
        path: &str,
        log: &[Event],
        batch: usize,
        reads_from: &mut Vec<u64>,
    ) -> Result<Option<Rebuilt>, Error> {
        Projector::open(path, "test")?.rebuild(
            &Projector::lock_rebuild(path)?,
            true,
            &mut |after, sink| {
                reads_from.push(after);
                stream_past(log, after, batch, sink)
            },
            &mut |_| {},
        )
    }

    /// A rebuild of the graph file at `path` from `log` in batches of two, owed by its caller,
    /// that fails at the second batch it is handed: its shadow is left holding the first.
    fn rebuild_interrupted_after_its_first_batch(
        path: &str,
        log: &[Event],
    ) -> Result<Option<Rebuilt>, Error> {
        let mut batches = 0;
        Projector::open(path, "test").unwrap().rebuild(
            &locked(path),
            true,
            &mut |after, sink| {
                stream_past(log, after, 2, &mut |events, head| {
                    batches += 1;
                    if batches > 1 {
                        return Err(Error(FIRST_BATCH_INTERRUPTED.to_string()));
                    }
                    sink(events, head)
                })
            },
            &mut |_| {},
        )
    }

    /// Of the events a rebuild folds from their own bytes, it passes over exactly those whose
    /// payload the fold rejects - of any type the fold reads a payload for - recording each in the
    /// `applied` ledger so the rebuilt file owes nothing for them, and its cause, the judge's text,
    /// in `passed_over`; every other such event folds.
    #[test]
    fn a_rebuild_passes_over_only_a_payload_the_fold_rejects_recording_its_cause() {
        let malformed = crate::test_support::POISON;
        let mistyped: &[u8] = br#"{"unit": 5}"#;
        let trailing: &[u8] = br#"{"id":"d-trailing","governs":["t.rs"]} x"#;
        let log = [
            decision_at("d1", "a.rs", 1),
            event_at(TYPE_DECISION_MADE, malformed, 2),
            event_at(TYPE_UNIT_INTEGRATED, mistyped, 3),
            event_at(TYPE_DECISION_MADE, trailing, 4),
            decision_at("d5", "b.rs", 5),
        ];
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();
        drop(Projector::open(path, "test").unwrap());

        let rebuilt = rebuild_in_batches(path, &log, 10, &mut Vec::new()).unwrap();
        let p = Projector::open(path, "test").unwrap();
        assert_eq!(
            (
                rebuilt.map(|r| r.pruned),
                applied_through(&p, 5),
                crate::test_support::passed_over_rows(path),
                live_governs(&p)
                    .into_iter()
                    .map(|g| g.1)
                    .collect::<Vec<_>>(),
            ),
            (
                Some(PruneStats::default()),
                vec![true; 5],
                vec![
                    crate::test_support::rejected_row(&log[1]),
                    crate::test_support::rejected_row(&log[2]),
                    crate::test_support::rejected_row(&log[3]),
                ],
                vec!["a.rs".to_string(), "b.rs".to_string()]
            ),
            "the three rejected payloads (malformed, mistyped, trailing bytes) are passed over \
             with their causes, recorded as folded, and the rest fold"
        );
        drop(p);
        assert_eq!(
            (
                rebuild_in_batches(path, &[decision_at("d6", "c.rs", 6)], 10, &mut Vec::new())
                    .unwrap()
                    .map(|r| r.pruned),
                crate::test_support::passed_over_rows(path).len(),
            ),
            (Some(PruneStats::default()), 0),
            "a rebuild that rejects nothing passes nothing over"
        );
    }

    /// A rebuild folds every lesson and finding the log holds as a node, one naming no location
    /// among them - the emit guard refuses such a record at emit only - and a finding naming its
    /// file is reached from that file (op-105-every-finding-reaches-the-graph).
    #[test]
    fn a_rebuild_folds_a_location_free_record_as_a_node() {
        let log = [
            event_at(
                TYPE_REVIEW_FINDING,
                br#"{"id":"f-nowhere","summary":"s"}"#,
                1,
            ),
            event_at(
                TYPE_LESSON_LEARNED,
                br#"{"id":"l-nowhere","summary":"s","about":[]}"#,
                2,
            ),
            event_at(
                TYPE_REVIEW_FINDING,
                br#"{"id":"f-file","summary":"s","file":"src/a.rs","line":3}"#,
                3,
            ),
        ];
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();
        drop(Projector::open(path, "test").unwrap());

        let rebuilt = rebuild_in_batches(path, &log, 10, &mut Vec::new()).unwrap();
        let p = Projector::open(path, "test").unwrap();
        let g = p.whole().unwrap();
        let mut nodes: Vec<(&str, &str)> = g
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), n.kind.as_str()))
            .collect();
        nodes.sort_unstable();
        let about: Vec<(&str, &str)> = g
            .edges
            .iter()
            .filter(|x| x.rel == REL_ABOUT)
            .map(|x| (x.from.as_str(), x.to.as_str()))
            .collect();
        assert_eq!(
            (
                rebuilt.map(|r| r.pruned),
                applied_through(&p, 3),
                crate::test_support::passed_over_rows(path),
                nodes,
                about,
            ),
            (
                Some(PruneStats::default()),
                vec![true; 3],
                vec![],
                vec![
                    ("f-file", KIND_FINDING),
                    ("f-nowhere", KIND_FINDING),
                    ("l-nowhere", KIND_LESSON),
                    ("src/a.rs", KIND_ARTIFACT),
                ],
                vec![("f-file", "src/a.rs")],
            ),
            "every record folds as a node, none passed over, and the file a finding names is ABOUT it"
        );
    }

    /// What a rebuild passed over is recorded by its fold of the shadow and by its tail into the
    /// live file alike: two rejected payloads the log held when it started, carried by the swap,
    /// and one it gained meanwhile, each with its cause.
    #[test]
    fn a_rebuild_records_what_its_fold_and_its_tail_each_passed_over() {
        let before = [
            event_at(TYPE_DECISION_MADE, crate::test_support::POISON, 1),
            decision_at("d2", "a.rs", 2),
            rejected_lesson_at(3),
        ];
        let gained = [event_at(TYPE_REVIEW_FINDING, b"7", 4)];
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();
        drop(Projector::open(path, "test").unwrap());
        let mut reads = 0;
        let rebuilt = Projector::open(path, "test")
            .unwrap()
            .rebuild(
                &locked(path),
                true,
                &mut |after, sink| {
                    reads += 1;
                    let log: &[Event] = if reads == 1 { &before } else { &gained };
                    stream_past(log, after, 10, sink)
                },
                &mut |_| {},
            )
            .unwrap();
        let p = Projector::open(path, "test").unwrap();
        assert_eq!(
            (
                rebuilt.map(|r| r.pruned),
                applied_through(&p, 4),
                crate::test_support::passed_over_rows(path),
            ),
            (
                Some(PruneStats::default()),
                vec![true; 4],
                vec![
                    crate::test_support::rejected_row(&before[0]),
                    crate::test_support::rejected_row(&before[2]),
                    crate::test_support::rejected_row(&gained[0])
                ],
            ),
            "two passed over by the fold and one by the tail, each recorded as folded"
        );
    }

    /// The log [`catching_up_over_a_log_folds_what_a_rebuild_of_it_folds`] folds both ways: a
    /// decision at 1 that folds, a decision at 2 and a lesson at 3 whose own payloads the judge
    /// rejects, and a decision at 4 that folds.
    fn passing_over_log() -> [Event; 4] {
        [
            decision_at("d1", "a.rs", 1),
            crate::test_support::poisoned_decision_at(2),
            rejected_lesson_at(3),
            decision_at("d4", "d.rs", 4),
        ]
    }

    /// `log`, a [`passing_over_log`], rebuilt into a fresh graph file, as setup rebuilds it: the
    /// rebuild completes, and the file's directory and path are handed back.
    fn rebuilt_passing_over(log: &[Event]) -> (tempfile::TempDir, String) {
        let (dir, path) = crate::test_support::temp_graph_db();
        assert_eq!(
            crate::test_support::rebuild_answer(&path, true, &mut |after, sink| stream_past(
                log, after, 10, sink
            ),),
            crate::test_support::clean_rebuild(),
            "the rebuild completes, passing every position it cannot fold over"
        );
        (dir, path)
    }

    /// Spec 124, the direct fold's own-position answer when a batch holds two of its own positions
    /// already passed over, each with a different cause, beside one that folds: the batch commits
    /// the position that folds and answers the FIRST passed-over position's cause.
    #[test]
    fn a_direct_batch_meeting_two_passed_over_positions_answers_the_first_ones_cause() {
        use crate::contextgraph::Fold;
        let (_dir, path) = crate::test_support::temp_graph_db();
        let log = [
            crate::test_support::poisoned_decision_at(1),
            rejected_lesson_at(2),
            decision_at("d-folds", "g.rs", 3),
        ];
        assert_eq!(
            crate::test_support::rebuild_answer(&path, true, &mut |after, sink| stream_past(
                &log, after, 10, sink
            ),),
            crate::test_support::clean_rebuild()
        );
        let p = Projector::open(&path, "test").unwrap();
        let (first, second) = (
            crate::test_support::rejected_row(&log[0]),
            crate::test_support::rejected_row(&log[1]),
        );
        let batch = [
            log[0].clone(),
            log[1].clone(),
            decision_at("d-commits", "h.rs", 4),
        ];
        assert_eq!(
            (
                crate::test_support::passed_over_rows(&path),
                crate::test_support::fold_of(&p, &batch),
                applied(&p, 4),
            ),
            (
                vec![first.clone(), second],
                Fold::NotFolded(crate::test_support::graph_error(&first.1)),
                true,
            ),
            "the batch answers the first of its passed-over positions' causes and commits the rest"
        );
    }

    /// A rebuild that meets a storage error folding an event whose payload the fold reads is not a
    /// rejection: the error propagates, nothing marks the event folded, the live file is untouched,
    /// and the next rebuild resumes from the last committed batch and folds it.
    #[test]
    fn a_storage_error_in_a_rebuild_propagates_and_the_next_rebuild_resumes_and_folds_it() {
        let log = [
            decision_at("d1", "a.rs", 1),
            decision_at(crate::test_support::D_BOOM, "b.rs", 2),
            decision_at("d3", "c.rs", 3),
        ];
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();
        drop(Projector::open(path, "test").unwrap());
        let shadow = shadow_of(path);
        drop(Projector::open(&shadow, "test").unwrap());
        // The shadow a rebuild opened its rebuild state in and folded nothing into yet, which the
        // next rebuild resumes, with a storage error waiting at d-boom's node.
        Connection::open(&shadow)
            .unwrap()
            .execute_batch(REBUILD_STATE)
            .unwrap();
        crate::test_support::the_disk_gives_out_at_d_boom(Path::new(&shadow));

        let mut reads_from = Vec::new();
        let failed = rebuild_in_batches(path, &log, 1, &mut reads_from);
        assert_eq!(
            (
                failed.map_err(|e| e.to_string()),
                applied(&Projector::open(path, "test").unwrap(), 1),
            ),
            (
                Err(crate::test_support::graph_error(
                    crate::test_support::DISK_GAVE_OUT
                )),
                false
            ),
            "the storage error propagates and the live file is untouched"
        );
        crate::test_support::the_disk_recovers(Path::new(&shadow));
        let rebuilt = rebuild_in_batches(path, &log, 1, &mut reads_from).unwrap();
        let p = Projector::open(path, "test").unwrap();
        assert_eq!(
            (
                rebuilt,
                reads_from,
                live_governs(&p)
                    .into_iter()
                    .map(|g| g.1)
                    .collect::<Vec<_>>(),
            ),
            (
                Some(Rebuilt::default()),
                vec![0, 1, 3],
                vec!["b.rs".to_string(), "a.rs".to_string(), "c.rs".to_string()]
            ),
            "the next rebuild resumes past the committed batch and folds the event the error lost \
             (the governing edges listed by decision id: d-boom, d1, d3)"
        );
    }

    /// Spec 122, THE IMMEDIATE OPEN: the transaction frame takes the write lock at its open, so a
    /// frame meeting a write lock another connection holds fails there once its busy timeout
    /// passes - its body never run, and nothing written - as CONTENTION ([`FrameError::Contended`],
    /// spec 124) carrying SQLite's own text.
    #[test]
    fn a_transaction_meeting_a_held_write_lock_fails_at_its_open_before_its_body_runs() {
        let (_dir, path) = crate::test_support::temp_graph_db();
        drop(Projector::open(&path, "test").unwrap());
        let mut conn = connection_to(&path);
        conn.busy_timeout(std::time::Duration::from_millis(100))
            .unwrap();
        let ran = std::cell::Cell::new(false);
        let opened = crate::test_support::with_graph_locked(Path::new(&path), || {
            in_transaction(&mut conn, |tx| {
                ran.set(true);
                record_applied(tx, 1).map(drop)
            })
        });
        let written: i64 = conn
            .query_row("SELECT count(*) FROM applied", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            (
                opened.map_err(|e| (matches!(e, FrameError::Contended(_)), e.unframed().0)),
                ran.get(),
                written
            ),
            (
                Err((true, crate::test_support::DATABASE_IS_LOCKED.to_string())),
                false,
                0
            ),
            "the frame fails at its open, its body never run and nothing written"
        );
    }

    /// Hand `sink` the positions of `log`, in order, in batches of at most `batch`.
    fn positions_in(log: &[Event], batch: usize, sink: &mut PositionSink) -> Result<(), Error> {
        log.chunks(batch)
            .try_for_each(|b| sink(&crate::test_support::positions_of(b)))
    }

    /// The causes the file behind `p` records and the seeding's verdict once its cursor is seeded
    /// against `log` as `rigger setup` seeds it ([`crate::test_support::owed_once_seeded`]):
    /// `log`'s positions streamed in batches of two so a hole in a later batch is met too, each
    /// missing position's stamp read from `log`, and `log`'s last position handed as its head.
    fn owed_once_seeded_against(p: &Projector, log: &[Event]) -> (Vec<&'static str>, Seeding) {
        crate::test_support::owed_once_seeded(
            p,
            &mut |sink| positions_in(log, 2, sink),
            &mut |at| crate::test_support::stamp_in(log, at),
            crate::test_support::head_of(log),
        )
    }

    /// What `rigger setup` does to the `graph.db` at `path` over `log`: seed its cursor and read
    /// why it owes its rebuild - a cause the file records, or a lost fold its seeding finds - then
    /// pay that, reporting whether it rebuilt.
    fn rebuild_as_setup_does(path: &str, log: &[Event]) -> bool {
        let owed = crate::test_support::setup_owes(&owed_once_seeded_against(
            &Projector::open(path, "test").unwrap(),
            log,
        ));
        Projector::open(path, "test")
            .unwrap()
            .rebuild(
                &locked(path),
                owed,
                &mut |after, sink| stream_past(log, after, 10, sink),
                &mut |_| {},
            )
            .unwrap()
            .is_some()
    }

    /// A lost fold owed by the `graph.db` at `path`, then paid: a fresh projector on the file
    /// holds no record of the loss, its seeding against `log` - whose `lost` position, missing
    /// below an applied one, is older than the catch-up horizon - finds the prefix violated,
    /// creating no cursor and naming the lost fold as the cause, a rebuild from `log` folds the
    /// lost position and seeds the cursor at `log`'s last position, and the rebuilt file owes
    /// nothing and folds again.
    fn the_ledger_owes_the_lost_fold_and_a_rebuild_pays_it(path: &str, log: &[Event], lost: u64) {
        let p = Projector::open(path, "test").unwrap();
        assert_eq!(
            (
                owed_once_seeded_against(&p, log),
                crate::test_support::caught_up_at(Path::new(path)),
            ),
            (crate::test_support::LOST_FOLD_ONCE_SEEDED, None),
            "the file records nothing, and the ledger's old hole is the debt, seeding no cursor"
        );
        drop(p);
        let rebuilt = rebuild_as_setup_does(path, log);
        let p = Projector::open(path, "test").unwrap();
        assert_eq!(
            (
                rebuilt,
                applied(&p, lost),
                crate::test_support::caught_up_at(Path::new(path)),
                owed_once_seeded_against(&p, log),
            ),
            (
                true,
                true,
                crate::test_support::head_of(log),
                crate::test_support::NOTHING_OWED_ONCE_SEEDED
            ),
            "the rebuild folds the lost position, seeds the cursor at the log's last position and \
             pays the debt"
        );
        crate::test_support::folds(&p, &[decision_at("d9", "z.rs", 9)]);
    }

    /// A decision at each of `positions`, in order, each governing `a.rs`.
    fn decisions_at(positions: impl IntoIterator<Item = u64>) -> Vec<Event> {
        positions
            .into_iter()
            .map(|at| decision_at(&format!("d{at}"), "a.rs", at))
            .collect()
    }

    /// The seeding of `p`'s cursor at `now` against the positions `log` streams with their stamps,
    /// `log`'s last position handed as its head, as `rigger setup` seeds it.
    fn seeded_against(p: &Projector, log: &[Event], now: SystemTime) -> Seeding {
        p.seed_cursor(
            &mut |sink| positions_in(log, 2, sink),
            &mut |at| crate::test_support::stamp_in(log, at),
            crate::test_support::head_of(log),
            now,
        )
        .unwrap()
    }

    /// A fresh graph file whose ledger records each position of `folded`, seeded at `now` against
    /// `log` ([`seeded_against`]): what the seeding answered, and where the file's cursor stands
    /// after it.
    fn seeding_over(log: &[Event], folded: &[u64], now: SystemTime) -> (Seeding, Option<u64>) {
        let (_dir, path) = crate::test_support::temp_graph_db();
        let p = Projector::open(&path, "test").unwrap();
        crate::test_support::folds(&p, &decisions_at(folded.iter().copied()));
        let seeding = seeded_against(&p, log, now);
        (seeding, crate::test_support::caught_up_at(Path::new(&path)))
    }

    /// Decisions at positions 1 through `last`, each stamped as recorded at [`recorded`]: the log
    /// the seeding tests hand, whose holes are as old as the seeding's clock is past that instant.
    fn decisions_through(last: u64) -> Vec<Event> {
        crate::test_support::restamped(&decisions_at(1..=last), |_| recorded())
    }

    /// When the events of [`decisions_through`] are recorded.
    fn recorded() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000)
    }

    /// The clock at which the events of [`decisions_through`], recorded at [`recorded`], are `age`
    /// old.
    fn at_age(age: Duration) -> SystemTime {
        recorded() + age
    }

    /// THE CURSOR AND ITS SEEDING (spec 124): a file without `caught_up` whose ledger misses no
    /// position below its highest applied one is seeded there, and an empty ledger beside an empty
    /// log at 0; a position missing above it is no hole of the prefix, however old. Holes below it
    /// no older than the catch-up horizon - at the horizon exactly, or stamped after the seeding's
    /// clock - seed the cursor just below the lowest of them. One older than the horizon by a
    /// nanosecond, beside a young one too, or one the lookup answers no stamp for, as setup's
    /// lookup does on the server-backed log, whose stamps are taken on another host's clock,
    /// violates the prefix, as an empty ledger beside a non-empty log does, even one recorded at
    /// the seeding's own clock, a non-empty ledger beside an empty log, and a highest applied
    /// position above the log's head, as when the log is replaced under a standing graph file: no
    /// cursor is created and the seeding says the file is lost.
    #[test]
    fn a_file_without_caught_up_is_seeded_at_its_ledger_s_prefix_and_never_over_an_old_hole() {
        let log = decisions_through(4);
        let past = CATCH_UP_HORIZON + Duration::from_nanos(1);
        let mut mixed = log.clone();
        mixed[2].recorded_at = recorded() - past;
        let lost = (Seeding::Ran { lost: true }, None);
        assert_eq!(
            [
                seeding_over(&log, &[1, 2, 3, 4], at_age(past)),
                seeding_over(&log, &[1, 2], at_age(past)),
                seeding_over(&log, &[1, 3, 4], at_age(CATCH_UP_HORIZON)),
                seeding_over(&log, &[1, 4], at_age(CATCH_UP_HORIZON)),
                seeding_over(&log, &[1, 3, 4], recorded() - Duration::from_secs(1)),
                seeding_over(&[], &[], recorded()),
                seeding_over(&log[..3], &[1, 2, 3], recorded()),
                seeding_over(&log, &[1, 3, 4], at_age(past)),
                seeding_over(&mixed, &[1, 4], recorded()),
                seeding_over(&log, &[], recorded()),
                seeding_over(&[], &[1, 2], recorded()),
                seeding_over(&log[..3], &[1, 2, 3, 4], recorded()),
            ],
            [
                (Seeding::Ran { lost: false }, Some(4)),
                (Seeding::Ran { lost: false }, Some(2)),
                (Seeding::Ran { lost: false }, Some(1)),
                (Seeding::Ran { lost: false }, Some(1)),
                (Seeding::Ran { lost: false }, Some(1)),
                (Seeding::Ran { lost: false }, Some(0)),
                (Seeding::Ran { lost: false }, Some(3)),
                lost.clone(),
                lost.clone(),
                lost.clone(),
                lost.clone(),
                lost.clone(),
            ],
            "the cursor stands at the highest applied position, zero beside an empty log, or just \
             below the lowest young hole, and an old hole, beside a young one too, an empty ledger \
             beside a non-empty log, a ledger beside an empty log or a ledger past the log's head \
             creates none"
        );
        let (_dir, path) = crate::test_support::temp_graph_db();
        let p = Projector::open(&path, "test").unwrap();
        crate::test_support::folds(&p, &[log[0].clone(), log[2].clone()]);
        assert_eq!(
            (
                p.seed_cursor(
                    &mut |sink| positions_in(&log, 2, sink),
                    &mut |_| Ok(None),
                    crate::test_support::head_of(&log),
                    recorded(),
                )
                .unwrap(),
                crate::test_support::caught_up_at(Path::new(&path)),
            ),
            lost,
            "a hole the lookup answers no stamp for is old"
        );
    }

    /// The seeding of `p`'s cursor at a day past [`recorded`], handed `head` as the log's head, by a
    /// caller whose positions and stamps a seeded file never reads.
    fn seeded_unread(p: &Projector, head: Option<Position>) -> Seeding {
        p.seed_cursor(
            &mut |_| panic!("a seeded file reads no positions"),
            &mut |_| panic!("a seeded file reads no stamp"),
            head,
            at_age(Duration::from_secs(86_400)),
        )
        .unwrap()
    }

    /// The seeding runs once per file: a file already carrying `caught_up` is not read - neither
    /// its log's positions nor a stamp - and keeps its cursor at 2, the hole at position 3 above
    /// the cursor and below the highest position its ledger records included; its verdict finds
    /// nothing lost and the file records no cause.
    #[test]
    fn a_file_carrying_caught_up_is_never_read_again_and_keeps_its_cursor() {
        let log = decisions_through(4);
        let (_dir, path) = crate::test_support::temp_graph_db();
        let p = Projector::open(&path, "test").unwrap();
        crate::test_support::folds(&p, &log[..2]);
        assert_eq!(
            owed_once_seeded_against(&p, &log[..2]),
            crate::test_support::NOTHING_OWED_ONCE_SEEDED
        );
        crate::test_support::folds(&p, &log[3..]);
        let seeding = seeded_unread(&p, crate::test_support::head_of(&log));
        assert_eq!(
            (
                seeding,
                crate::test_support::caught_up_at(Path::new(&path)),
                p.owed_because().unwrap(),
            ),
            (Seeding::Ran { lost: false }, Some(2), Vec::<&str>::new()),
            "the seeded file is not read, keeps its cursor and owes nothing"
        );
    }

    /// THE TWO WRITES MEETING A LOCK (spec 124): a seeding whose transaction meets another
    /// writer's lock past the busy timeout is skipped - it reads nothing and creates no cursor -
    /// and what the file owes is what it records, read with no lock, the owed mark standing beside
    /// it no cause; the next seeding, the lock released, seeds it.
    #[test]
    fn a_seeding_that_meets_the_write_lock_is_skipped_reading_and_writing_nothing() {
        let log = decisions_through(4);
        let (dir, path) = crate::test_support::temp_graph_db();
        let p = Projector::open(&path, "test").unwrap();
        crate::test_support::folds(&p, &log);
        impatient(&p);
        let seeding = crate::test_support::with_graph_locked(Path::new(&path), || {
            p.seed_cursor(
                &mut |_| panic!("a skipped seeding reads no positions"),
                &mut |_| panic!("a skipped seeding reads no stamp"),
                crate::test_support::head_of(&log),
                recorded(),
            )
            .unwrap()
        });
        crate::test_support::mark_owed_in(dir.path());
        assert_eq!(
            (
                seeding,
                crate::test_support::caught_up_at(Path::new(&path)),
                p.owed_because().unwrap(),
            ),
            (Seeding::Skipped, None, Vec::<&str>::new()),
            "the contended seeding is skipped, and the file owes what it records: the mark is no \
             cause"
        );
        assert_eq!(
            (
                owed_once_seeded_against(&p, &log),
                crate::test_support::caught_up_at(Path::new(&path))
            ),
            (crate::test_support::NOTHING_OWED_ONCE_SEEDED, Some(4)),
            "the next seeding, the lock released, seeds the file"
        );
    }

    /// THE REBUILD (spec 124): the rebuild's shadow open creates `caught_up` at the shadow's
    /// rebuild cursor - zero in a fresh shadow, the cursor a resumed one stands at when an older
    /// binary left it without one - and the drop that finishes the rebuild sets it to the final
    /// rebuild cursor.
    #[test]
    fn the_rebuild_seeds_caught_up_at_its_shadow_open_and_sets_it_at_its_drop() {
        let log = decisions_through(6);
        let (_dir, path) = crate::test_support::temp_graph_db();
        let shadow = shadow_of(&path);
        drop(Projector::open(&path, "test").unwrap());
        let first = rebuild_interrupted_after_its_first_batch(&path, &log).map_err(|e| e.0);
        let fresh = crate::test_support::caught_up_at(Path::new(&shadow));
        crate::test_support::drop_the_cursor(Path::new(&shadow));
        let resumed = rebuild_interrupted_after_its_first_batch(&path, &log).map_err(|e| e.0);
        let older = crate::test_support::caught_up_at(Path::new(&shadow));
        let finished = rebuild_in_batches(&path, &log, 2, &mut Vec::new()).unwrap();
        assert_eq!(
            (
                first,
                fresh,
                resumed,
                older,
                finished.is_some(),
                Path::new(&shadow).exists(),
                crate::test_support::caught_up_at(Path::new(&path)),
                rebuild_state(&path),
            ),
            (
                Err(FIRST_BATCH_INTERRUPTED.to_string()),
                Some(0),
                Err(FIRST_BATCH_INTERRUPTED.to_string()),
                Some(2),
                true,
                false,
                Some(6),
                (false, false),
            ),
            "the shadow open seeds caught_up at its rebuild cursor, the drop at the final one"
        );
    }

    /// The event log a [`catching_up`] graph file folds forward from: the store beside the file,
    /// every read of which is counted.
    type CountedLog = Arc<crate::test_support::ReadCountingStore<'static>>;

    /// The path of the `events.db` beside the graph file at `path`: where a store keeps the log
    /// that file folds.
    fn events_beside(path: &str) -> String {
        Path::new(path)
            .with_file_name("events.db")
            .to_str()
            .unwrap()
            .to_string()
    }

    /// The opener of the log beside the graph file at `path` ([`events_beside`]), unscoped: each
    /// call opens a store of its own on it, every project's streams under their full names.
    fn the_unscoped_log_beside(
        path: &str,
    ) -> impl Fn() -> Result<Arc<dyn EventStore>, Error> + Send + Sync + 'static {
        let events = events_beside(path);
        move || {
            rigger_store_sqlite::eventstore::sqlite::Store::open(&events)
                .map(|store| Arc::new(store) as Arc<dyn EventStore>)
                .map_err(be)
        }
    }

    /// `log` scoped to the project `test` ([`Namespaced::owning`]), as the write open scopes the
    /// log it hands a projector under that project.
    ///
    /// [`Namespaced::owning`]: rigger_store_sqlite::eventstore::namespace::Namespaced::owning
    fn scoped_to_test(log: Arc<dyn EventStore>) -> Arc<dyn EventStore> {
        Arc::new(rigger_store_sqlite::eventstore::namespace::Namespaced::owning(log, "test"))
    }

    /// The opener of the log beside the graph file at `path` ([`the_unscoped_log_beside`]) scoped
    /// to the project `test` ([`scoped_to_test`]): each call opens a store of its own on it.
    fn the_log_beside(
        path: &str,
    ) -> impl Fn() -> Result<Arc<dyn EventStore>, Error> + Send + Sync + 'static {
        let open = the_unscoped_log_beside(path);
        move || open().map(scoped_to_test)
    }

    /// A projector on the graph file at `path` under the project `test`, handed no log, whose
    /// `caught_up` cursor is seeded at 0 beside an empty log, as `rigger setup` seeds a new file.
    fn seeded_at(path: &str) -> Projector {
        let p = Projector::open(path, "test").unwrap();
        assert_eq!(
            seeded_against(&p, &[], recorded()),
            Seeding::Ran { lost: false }
        );
        p
    }

    /// A fresh graph file seeded as `rigger setup` seeds a new file ([`seeded_at`]): the directory
    /// that keeps it, its path and the projector on it.
    fn seeded_graph() -> (tempfile::TempDir, String, Projector) {
        let (dir, path) = crate::test_support::temp_graph_db();
        let p = seeded_at(&path);
        (dir, path, p)
    }

    /// A [`seeded_graph`] handed the log beside it ([`events_beside`]) to fold forward from,
    /// scoped to its project ([`scoped_to_test`]), and the store under that scope, every read of
    /// which is counted.
    fn catching_up() -> (tempfile::TempDir, String, Projector, CountedLog) {
        let (dir, path, p) = seeded_graph();
        let log: CountedLog = Arc::new(crate::test_support::ReadCountingStore::owning(
            rigger_store_sqlite::eventstore::sqlite::Store::open(&events_beside(&path)).unwrap(),
        ));
        let handed = scoped_to_test(Arc::clone(&log) as Arc<dyn EventStore>);
        let p = p.with_log(move || Ok(Arc::clone(&handed)));
        (dir, path, p, log)
    }

    /// Append `events`, in order, to `stream` of the project `project` in `log`, as that project's
    /// writer appends them: through the namespace that scopes its streams.
    fn appended_to(log: &dyn EventStore, project: &str, stream: &str, events: &[Event]) {
        rigger_store_sqlite::eventstore::namespace::Namespaced::new(log, project)
            .append(stream, crate::eventstore::ExpectedRevision::Any, events)
            .unwrap();
    }

    /// Append `events`, in order, to the run stream of the project `test` in `log`
    /// ([`appended_to`]): the events a graph file under that project folds.
    fn logged(log: &dyn EventStore, events: &[Event]) {
        appended_to(log, "test", rigger_domain::run::STREAM, events);
    }

    /// What `p` answers to [`Projector::catch_up`] called until an answer is not
    /// [`CatchUp::Advanced`], each error as its text: at most 16 calls, so a file that never stops
    /// advancing ends the test.
    fn answers(p: &Projector) -> Vec<Result<CatchUp, String>> {
        let mut answers = Vec::new();
        while answers.len() < 16 {
            let answer = p.catch_up().map_err(|e| e.0);
            let advanced = matches!(answer, Ok(CatchUp::Advanced { .. }));
            answers.push(answer);
            if !advanced {
                break;
            }
        }
        answers
    }

    /// What [`answers`] reads of a file whose catch-up folds the one position past its cursor and
    /// then meets the log's head.
    fn folds_one_then_meets_the_head() -> Vec<Result<CatchUp, String>> {
        vec![Ok(CatchUp::Advanced { recorded: 1 }), Ok(CatchUp::AtHead)]
    }

    /// The page read of the log a catch-up call makes from `from`, handed `materialized` events of
    /// its project's streams.
    fn page_read(from: Position, materialized: usize) -> crate::test_support::CountedRead {
        crate::test_support::CountedRead::AllPage {
            from,
            max: FOLD_BATCH,
            materialized,
        }
    }

    /// Each `(from, materialized)` of `reads` as the page read it names ([`page_read`]).
    fn page_reads(reads: &[(Position, usize)]) -> Vec<crate::test_support::CountedRead> {
        reads.iter().map(|&(from, n)| page_read(from, n)).collect()
    }

    /// Another writer of the graph file at `path`: a projector of its own under the project `test`,
    /// folding forward from a store of its own on the log beside the file, scoped to that project
    /// ([`the_log_beside`]).
    fn another_writer(path: &str) -> Projector {
        Projector::open(path, "test")
            .unwrap()
            .with_log(the_log_beside(path))
    }

    /// Make `p`'s connection give up on another writer's write lock after a millisecond.
    fn impatient(p: &Projector) {
        p.conn
            .lock()
            .unwrap()
            .busy_timeout(Duration::from_millis(1))
            .unwrap();
    }

    /// What a test makes stand in, or beside, the graph file at `path` before the file is asked.
    type Stand = fn(&str);

    /// The graph file at `path` holding the cursor a swapped-in rebuild's tail is owed past.
    fn a_standing_tail(path: &str) {
        connection_to(path).execute_batch(REBUILD_STATE).unwrap();
    }

    /// The graph file at `path` recording the fold rule before the current one over a decision it
    /// folded: a file at an older rule that folded nothing takes the current one on open, so the
    /// decision is folded first.
    fn folded_under_an_older_rule(path: &str) {
        crate::test_support::folds(
            &Projector::open(path, "test").unwrap(),
            &decisions_through(1),
        );
        crate::test_support::stamp_user_version(Path::new(path), PROJECTION_VERSION - 1)
    }

    /// The graph file at `path` carrying the owed mark an older binary's lost fold left beside it.
    fn carrying_an_owed_mark(path: &str) {
        std::fs::write(owed_mark(path).unwrap(), b"").unwrap()
    }

    /// The graph file at `path` without its `caught_up` cursor.
    fn without_a_cursor(path: &str) {
        crate::test_support::drop_the_cursor(Path::new(path))
    }

    /// The graph file at `path` whose `caught_up` cursor was recorded beside a server's log, not
    /// the log its projector folds.
    fn beside_another_log(path: &str) {
        crate::test_support::record_the_cursor_beside(Path::new(path), "shared:5cfaa9387d2839e1")
    }

    /// The graph file at `path` whose `caught_up` cursor records no log, as a cursor whose table
    /// predates that identity.
    fn beside_no_log(path: &str) {
        crate::test_support::drop_the_cursor_s_log(Path::new(path))
    }

    /// Spec 124, ONE PROCEDURE: a foreign non-entry folds in order. A decision past the cursor and
    /// one superseding it, another project's position between them, fold as log order folds them -
    /// the superseded decision's `GOVERNS` edge invalidated, the superseding one's live - in one
    /// cut through the page's last position; the other project's position is neither read nor
    /// folded, and the next call meets the head.
    #[test]
    fn a_foreign_decision_folds_in_log_order_past_another_project_s_position() {
        let (_dir, path, p, log) = catching_up();
        logged(&*log, &[decision_at("d1", "a.rs", 1)]);
        appended_to(
            &*log,
            "other",
            rigger_domain::run::STREAM,
            &[decision_at("o2", "o.rs", 2)],
        );
        let superseding = crate::test_support::decision_json("d3", "x", &["b.rs"], "d1");
        logged(
            &*log,
            &[Event::new(
                TYPE_DECISION_MADE,
                serde_json::to_vec(&superseding).unwrap(),
            )],
        );
        assert_eq!(
            (
                answers(&p),
                live_governs(&p)
                    .into_iter()
                    .map(|g| (g.0, g.1))
                    .collect::<Vec<_>>(),
                applied_through(&p, 3),
                crate::test_support::caught_up_at(Path::new(&path)),
                log.reads(),
            ),
            (
                vec![Ok(CatchUp::Advanced { recorded: 2 }), Ok(CatchUp::AtHead)],
                vec![("d3".to_string(), "b.rs".to_string())],
                vec![true, false, true],
                Some(3),
                page_reads(&[(0, 2), (3, 0)]),
            ),
            "the decisions fold in log order, the other project's position unread"
        );
    }

    /// Spec 124, ONE PROCEDURE: each call reads one page of the log past the cursor, of at most
    /// [`FOLD_BATCH`] events, narrowed to the project's run stream, and folds its first cut
    /// ([`fold_transactions`]) - through the page's last position when the cut covers every
    /// narrowed event, else through the cut's last - until a call meets
    /// no event past the cursor ([`CatchUp::AtHead`]). An empty log is at its head; a page of
    /// another stream's events alone moves the cursor past them, folding nothing; a record whose
    /// payload the judge rejects is passed over `payload rejected`; a gap longer than a page is
    /// read one page per call.
    #[test]
    fn each_catch_up_call_folds_one_cut_of_one_page_past_the_cursor_until_the_head() {
        let canary = rigger_domain::canary::STREAM;
        let run = rigger_domain::run::STREAM;
        let advanced = |recorded| CatchUp::Advanced { recorded };
        // Each case: the batches the log is appended, each to a stream of the project `test`; what
        // the calls answer; their page reads, each as where it read from and how many events it
        // handed back; where the cursor stands after them; and what was passed over.
        type Case = (
            &'static str,
            Vec<(&'static str, Vec<Event>)>,
            Vec<CatchUp>,
            Vec<(Position, usize)>,
            Position,
            Vec<(u64, String)>,
        );
        let cases: [Case; 5] = [
            (
                "an empty log",
                vec![],
                vec![CatchUp::AtHead],
                vec![(0, 0)],
                0,
                vec![],
            ),
            (
                "a page of another stream's events alone",
                vec![(canary, decisions_at(1..=2))],
                vec![advanced(0), CatchUp::AtHead],
                vec![(0, 2), (2, 0)],
                2,
                vec![],
            ),
            (
                "a run event before another stream's",
                vec![(run, decisions_at([1])), (canary, decisions_at([2]))],
                vec![advanced(1), CatchUp::AtHead],
                vec![(0, 2), (2, 0)],
                2,
                vec![],
            ),
            (
                "a rejected payload between two decisions, another stream's event last",
                vec![
                    (
                        run,
                        vec![
                            decision_at("d1", "a.rs", 1),
                            crate::test_support::poisoned_decision_at(2),
                            decision_at("d3", "c.rs", 3),
                        ],
                    ),
                    (canary, decisions_at([4])),
                ],
                vec![advanced(3), CatchUp::AtHead],
                vec![(0, 4), (4, 0)],
                4,
                vec![crate::test_support::rejected_row(
                    &crate::test_support::poisoned_decision_at(2),
                )],
            ),
            (
                "a gap longer than a page",
                vec![(run, decisions_at(1..=258))],
                vec![advanced(256), advanced(2), CatchUp::AtHead],
                vec![(0, 256), (256, 2), (258, 0)],
                258,
                vec![],
            ),
        ];
        for (case, batches, calls, reads, cursor, passed) in cases {
            let (_dir, path, p, log) = catching_up();
            for (stream, events) in &batches {
                appended_to(&*log, "test", stream, events);
            }
            assert_eq!(
                (
                    answers(&p),
                    log.reads(),
                    crate::test_support::caught_up_at(Path::new(&path)),
                    crate::test_support::passed_over_rows(&path),
                ),
                (
                    calls.into_iter().map(Ok).collect::<Vec<_>>(),
                    page_reads(&reads),
                    Some(cursor),
                    passed,
                ),
                "{case}: the calls' answers, their page reads, the cursor and what was passed over"
            );
        }
    }

    /// Spec 124, ONE PROCEDURE: catching a file up over a log folds what a rebuild of that log
    /// folds - a decision and a lesson whose payloads the judge rejects each passed over with its
    /// cause, the decisions around them folded - in one call.
    #[test]
    fn catching_up_over_a_log_folds_what_a_rebuild_of_it_folds() {
        let events = passing_over_log();
        let (_rebuilt_dir, rebuilt) = rebuilt_passing_over(&events);
        let rebuilt_p = Projector::open(&rebuilt, "test").unwrap();
        let (_dir, path, p, log) = catching_up();
        logged(&*log, &events);
        assert_eq!(
            (answers(&p), log.reads()),
            (
                vec![Ok(CatchUp::Advanced { recorded: 4 }), Ok(CatchUp::AtHead)],
                page_reads(&[(0, 4), (4, 0)]),
            ),
            "the four positions fold in one call"
        );
        assert_eq!(
            (
                crate::test_support::passed_over_rows(&path),
                applied_through(&p, 4),
                crate::test_support::whole_json(&p),
            ),
            (
                crate::test_support::passed_over_rows(&rebuilt),
                applied_through(&rebuilt_p, 4),
                crate::test_support::whole_json(&rebuilt_p),
            ),
            "the caught-up file holds what the rebuilt one holds"
        );
    }

    /// Spec 124, ONE PROCEDURE: a file whose rebuild seeds or repairs its cursor folds no catch-up
    /// of its own ([`CatchUp::Inert`]), opening no log and writing nothing - one holding a
    /// rebuild's state, one recording an older fold rule over what it folded, one whose cursor
    /// was recorded beside another log or beside none, one without `caught_up` - and of two such
    /// causes standing together the first in that order is the answer; the owed mark standing
    /// beside the file changes no answer.
    #[test]
    fn catch_up_on_a_file_its_rebuild_seeds_or_repairs_opens_no_log_and_answers_inert() {
        let cases: [(&str, &[Stand], Inert, Option<Position>); 9] = [
            (
                "a rebuild's state",
                &[a_standing_tail],
                Inert::RebuildState,
                Some(0),
            ),
            (
                "an older fold rule",
                &[folded_under_an_older_rule],
                Inert::OlderRule,
                Some(0),
            ),
            (
                "a cursor recorded beside another log",
                &[beside_another_log],
                Inert::LogSwitched,
                Some(0),
            ),
            (
                "a cursor recorded beside no log",
                &[beside_no_log],
                Inert::LogSwitched,
                Some(0),
            ),
            ("no cursor", &[without_a_cursor], Inert::NoCursor, None),
            (
                "an older fold rule beside a cursor recorded beside another log",
                &[beside_another_log, folded_under_an_older_rule],
                Inert::OlderRule,
                Some(0),
            ),
            (
                "a rebuild's state beside an older fold rule",
                &[folded_under_an_older_rule, a_standing_tail],
                Inert::RebuildState,
                Some(0),
            ),
            (
                "an older fold rule beside the owed mark",
                &[folded_under_an_older_rule, carrying_an_owed_mark],
                Inert::OlderRule,
                Some(0),
            ),
            (
                "the owed mark beside no cursor",
                &[without_a_cursor, carrying_an_owed_mark],
                Inert::NoCursor,
                None,
            ),
        ];
        for (case, stands, inert, cursor) in cases {
            let (_dir, path, p) = seeded_graph();
            let p = p.with_log(|| panic!("a file that folds no catch-up opens no log"));
            for stand in stands {
                stand(&path);
            }
            assert_eq!(
                (
                    p.catch_up().map_err(|e| e.0),
                    crate::test_support::caught_up_at(Path::new(&path)),
                ),
                (Ok(CatchUp::Inert(inert)), cursor),
                "{case}: the answer, and the cursor as it stood"
            );
        }
    }

    /// Spec 124, THE MARK: the owed mark an older binary's lost fold left beside a file carrying
    /// `caught_up` is no cause - the file's catch-up opens the log and folds the decision past its
    /// cursor, then answers it is at the head, as it would with no mark.
    #[test]
    fn catch_up_beside_the_owed_mark_folds_past_the_cursor() {
        let (_dir, path, p, log) = catching_up();
        carrying_an_owed_mark(&path);
        logged(&*log, &decisions_at([1]));
        assert_eq!(
            (
                answers(&p),
                crate::test_support::caught_up_at(Path::new(&path))
            ),
            (folds_one_then_meets_the_head(), Some(1)),
            "the marked file catches up past its cursor"
        );
    }

    /// Spec 124, ONE PROCEDURE: a cause another writer brings about between a call's page read and
    /// its cut - a rebuild's state, an older fold rule, the cursor recorded beside another log or
    /// beside none, the cursor dropped - is read again under the cut's write lock: the call answers
    /// it ([`CatchUp::Inert`]), folding nothing, leaving the cursor as it stood and reading no
    /// second page.
    #[test]
    fn catch_up_meeting_a_cause_under_its_write_lock_answers_inert_folding_nothing() {
        let cases: [(&str, Stand, Inert, Option<Position>); 5] = [
            (
                "a rebuild's state",
                a_standing_tail,
                Inert::RebuildState,
                Some(0),
            ),
            (
                "an older fold rule",
                folded_under_an_older_rule,
                Inert::OlderRule,
                Some(0),
            ),
            (
                "a cursor recorded beside another log",
                beside_another_log,
                Inert::LogSwitched,
                Some(0),
            ),
            (
                "a cursor recorded beside no log",
                beside_no_log,
                Inert::LogSwitched,
                Some(0),
            ),
            ("no cursor", without_a_cursor, Inert::NoCursor, None),
        ];
        for (case, stand, inert, cursor) in cases {
            let (_dir, path, p, log) = catching_up();
            logged(&*log, &decisions_at(1..=2));
            let standing = path.clone();
            log.after_call(0, move |_| stand(&standing));
            assert_eq!(
                (
                    p.catch_up().map_err(|e| e.0),
                    crate::test_support::caught_up_at(Path::new(&path)),
                    applied(&p, 2),
                    log.reads(),
                ),
                (
                    Ok(CatchUp::Inert(inert)),
                    cursor,
                    false,
                    page_reads(&[(0, 2)])
                ),
                "{case}: the answer, the cursor as it stood, nothing folded and one page read"
            );
        }
    }

    /// Spec 124, ONE PROCEDURE: a projector handed no log has none to fold forward from - its call
    /// answers the head, writing nothing.
    #[test]
    fn catch_up_handed_no_log_answers_at_head() {
        let (_dir, path, p) = seeded_graph();
        assert_eq!(
            (
                p.catch_up().map_err(|e| e.0),
                crate::test_support::caught_up_at(Path::new(&path)),
            ),
            (Ok(CatchUp::AtHead), Some(0))
        );
    }

    /// Spec 124, WIRING: a call folds the run stream of the log it is handed as that log presents
    /// it, laying no scope of its own over it - the write open hands the log already scoped to its
    /// project - so a log handed unscoped presents its unprefixed run stream as the project's, and
    /// that stream's event is what folds.
    #[test]
    fn catch_up_folds_the_run_stream_of_the_log_it_is_handed_laying_no_scope_over_it() {
        let (_dir, path, p) = seeded_graph();
        let p = p.with_log(the_unscoped_log_beside(&path));
        the_unscoped_log_beside(&path)()
            .unwrap()
            .append(
                rigger_domain::run::STREAM,
                crate::eventstore::ExpectedRevision::Any,
                &decisions_at([1]),
            )
            .unwrap();
        assert_eq!(
            (
                answers(&p),
                crate::test_support::caught_up_at(Path::new(&path)),
            ),
            (folds_one_then_meets_the_head(), Some(1)),
            "the handed log's unprefixed run event folds, the cursor past it"
        );
    }

    /// Another writer of the graph file at `path` ([`another_writer`]) catching it up the moment
    /// call `at` of `log` returns, insisting it folds `recorded` positions.
    fn caught_up_by_another_writer(log: &CountedLog, path: &str, at: usize, recorded: usize) {
        let q = another_writer(path);
        log.after_call(at, move |_| {
            assert_eq!(
                q.catch_up().map_err(|e| e.0),
                Ok(CatchUp::Advanced { recorded }),
                "the other writer catches the file up"
            );
        });
    }

    /// Another writer catching the graph file at `path` up past the cut of `log`'s first page read
    /// ([`caught_up_by_another_writer`]), then the log gaining a decision at 4.
    fn outrun_once(log: &CountedLog, path: &str) {
        caught_up_by_another_writer(log, path, 0, 3);
        log.after_call(0, |inner| logged(inner, &decisions_at([4])));
    }

    /// Spec 124, ONE PROCEDURE: a call's cut reads the cursor again under its write lock. Another
    /// writer's catch-up past the cut, or a swap lowering the cursor below the call's page, sends
    /// the call to read one page again from where the cursor then stands, and when that page's cut
    /// is outrun too the call answers [`CatchUp::Skipped`] at the position it read that page from,
    /// writing nothing. Another writer's
    /// direct fold of a position of the cut, or a cursor moved inside the cut, is no outrun: the
    /// call folds the rest, counting only the positions its own transaction recorded.
    #[test]
    fn catch_up_reads_one_page_again_when_another_writer_passes_its_cut_and_skips_the_second() {
        // How a case places another writer's act among the calls, given the log and the file.
        type Act = fn(&CountedLog, &str);
        // Each case: another writer's act; what the calls answer; their page reads, each as where
        // it read from and how many events it handed back; and where the cursor stands after them.
        type Case = (
            &'static str,
            Act,
            Vec<CatchUp>,
            Vec<(Position, usize)>,
            Position,
        );
        let advanced = |recorded| CatchUp::Advanced { recorded };
        let cases: [Case; 5] = [
            (
                "a catch-up past the cut, the log gaining a position",
                outrun_once,
                vec![advanced(1), CatchUp::AtHead],
                vec![(0, 3), (3, 1), (4, 0)],
                4,
            ),
            (
                "a catch-up past the cut read again too",
                |log, path| {
                    outrun_once(log, path);
                    caught_up_by_another_writer(log, path, 1, 1);
                },
                vec![CatchUp::Skipped { at: 3 }],
                vec![(0, 3), (3, 1)],
                4,
            ),
            (
                "a direct fold of a position of the cut",
                |log, path| {
                    let q = Projector::open(path, "test").unwrap();
                    log.after_call(0, move |_| {
                        crate::test_support::folds(&q, &decisions_at([2]))
                    });
                },
                vec![advanced(2), CatchUp::AtHead],
                vec![(0, 3), (3, 0)],
                3,
            ),
            (
                "the cursor moved inside the cut",
                |log, path| {
                    let q = Projector::open(path, "test").unwrap();
                    let at = path.to_string();
                    log.after_call(0, move |_| {
                        crate::test_support::folds(&q, &decisions_at(1..=2));
                        crate::test_support::move_the_cursor(Path::new(&at), 2);
                    });
                },
                vec![advanced(1), CatchUp::AtHead],
                vec![(0, 3), (3, 0)],
                3,
            ),
            (
                "a swap lowering the cursor below the call's page",
                |log, path| {
                    crate::test_support::folds(
                        &Projector::open(path, "test").unwrap(),
                        &decisions_at(1..=2),
                    );
                    crate::test_support::move_the_cursor(Path::new(path), 2);
                    let at = path.to_string();
                    log.after_call(0, move |_| {
                        crate::test_support::move_the_cursor(Path::new(&at), 0)
                    });
                },
                vec![advanced(1), CatchUp::AtHead],
                vec![(2, 1), (0, 3), (3, 0)],
                3,
            ),
        ];
        for (case, act, calls, reads, cursor) in cases {
            let (_dir, path, p, log) = catching_up();
            logged(&*log, &decisions_at(1..=3));
            act(&log, &path);
            assert_eq!(
                (
                    answers(&p),
                    log.reads(),
                    crate::test_support::caught_up_at(Path::new(&path)),
                ),
                (
                    calls.into_iter().map(Ok).collect::<Vec<_>>(),
                    page_reads(&reads),
                    Some(cursor),
                ),
                "{case}: the calls' answers, their page reads and the cursor"
            );
        }
    }

    /// Spec 124, THE TWO WRITES MEETING A LOCK: a call whose cut meets another writer's write lock
    /// past the busy timeout answers [`CatchUp::Skipped`] at the cursor it read from, folding
    /// nothing and leaving the cursor where it stood; the next call, the lock released, folds.
    #[test]
    fn catch_up_meeting_the_write_lock_is_skipped_at_its_cursor_writing_nothing() {
        let (_dir, path, p, log) = catching_up();
        logged(&*log, &decisions_at([1]));
        impatient(&p);
        let skipped = crate::test_support::with_graph_locked(Path::new(&path), || {
            p.catch_up().map_err(|e| e.0)
        });
        assert_eq!(
            (
                skipped,
                crate::test_support::caught_up_at(Path::new(&path)),
                applied(&p, 1),
            ),
            (Ok(CatchUp::Skipped { at: 0 }), Some(0), false),
            "the contended call is skipped, writing nothing"
        );
        assert_eq!(
            answers(&p),
            folds_one_then_meets_the_head(),
            "the next call, the lock released, folds"
        );
    }

    /// What the opener of the log [`a_catch_up_that_fails_answers_failed_at_its_cursor_writing_nothing`]
    /// hands answers the first time it is called.
    const LOG_UNOPENED: &str = "the log could not be opened";

    /// Spec 124, ONE PROCEDURE: a call that fails - the log's open, its page read or its cut's
    /// fold - answers [`CatchUp::Failed`] at the position it reads, or would read, its page from,
    /// the cursor it read, with the failure's text, writing nothing, at a cursor of 0 and above it
    /// alike. A failed open is tried again by the next call, and a log once opened is never opened
    /// again.
    #[test]
    fn a_catch_up_that_fails_answers_failed_at_its_cursor_writing_nothing() {
        let (_dir, path, p) = seeded_graph();
        let tries = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let tried = Arc::clone(&tries);
        let open = the_log_beside(&path);
        let p = p.with_log(move || {
            if tried.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                return Err(Error(LOG_UNOPENED.to_string()));
            }
            open()
        });
        assert_eq!(
            (
                [p.catch_up(), p.catch_up(), p.catch_up()].map(|a| a.map_err(|e| e.0)),
                tries.load(std::sync::atomic::Ordering::SeqCst),
            ),
            (
                [
                    Ok(CatchUp::Failed {
                        at: 0,
                        error: LOG_UNOPENED.to_string()
                    }),
                    Ok(CatchUp::AtHead),
                    Ok(CatchUp::AtHead),
                ],
                2,
            ),
            "the failed open is tried again, and the opened log kept"
        );

        // What fails the call, given its projector, its graph file, its log and the cursor it
        // reads from: the projector the call is made on, and the failure's text.
        type Failing = fn(Projector, &str, &CountedLog, Position) -> (Projector, String);
        let arms: [(&str, Failing); 3] = [
            ("the log's open", |p, _, _, _| {
                let p = p.with_log(|| Err(Error(LOG_UNOPENED.to_string())));
                (p, LOG_UNOPENED.to_string())
            }),
            ("its page read", |p, path, log, at| {
                crate::test_support::drop_the_log(Path::new(&events_beside(path)));
                let unread = scoped_to_test(Arc::clone(log) as Arc<dyn EventStore>)
                    .read_all_page(at, &crate::eventstore::Filter::default(), FOLD_BATCH)
                    .unwrap_err()
                    .to_string();
                (p, unread)
            }),
            ("its cut's fold", |p, path, log, at| {
                logged(
                    &**log,
                    &[decision_at(crate::test_support::D_BOOM, "b.rs", at + 1)],
                );
                crate::test_support::the_disk_gives_out_at_d_boom(Path::new(path));
                (p, crate::test_support::DISK_GAVE_OUT.to_string())
            }),
        ];
        for (arm, failing) in arms {
            for cursor in [0, 3] {
                let (_dir, path, p, log) = catching_up();
                logged(&*log, &decisions_at(1..=cursor));
                crate::test_support::move_the_cursor(Path::new(&path), cursor);
                let (p, error) = failing(p, &path, &log, cursor);
                assert_eq!(
                    (
                        answers(&p),
                        crate::test_support::caught_up_at(Path::new(&path)),
                        crate::test_support::passed_over_rows(&path),
                        applied(&p, cursor + 1),
                    ),
                    (
                        vec![Ok(CatchUp::Failed { at: cursor, error })],
                        Some(cursor),
                        vec![],
                        false,
                    ),
                    "{arm} failing at the cursor {cursor}: the call answers it there with the \
                     failure's text, writing nothing"
                );
            }
        }
    }

    /// Spec 124, ONE PROCEDURE: the one setter of a cursor never moves it backward - a position
    /// below where it stands leaves it standing, its own position keeps it, and one above moves it.
    #[test]
    fn set_cursor_moves_a_cursor_forward_only() {
        let mut conn = Connection::open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        create_cursor(&tx, 5, "").unwrap();
        let stands = [3, 5, 7].map(|at| {
            set_cursor(&tx, "caught_up", at).unwrap();
            cursor_at(&tx, "caught_up").unwrap().unwrap()
        });
        assert_eq!(stands, [5, 5, 7]);
    }

    /// Spec 124, THE CURSOR AND ITS SEEDING: a rebuild's cursor is read as every one-row cursor
    /// table is ([`cursor_at`]): where the rebuild state of the file holds it, and, on a file
    /// holding no rebuild state, the error saying so.
    #[test]
    fn a_rebuild_cursor_is_read_from_the_rebuild_state_and_its_absence_is_named() {
        let conn = Connection::open_in_memory().unwrap();
        let absent = rebuild_cursor(&conn).map_err(|e| e.0);
        conn.execute_batch(REBUILD_STATE).unwrap();
        conn.execute("UPDATE rebuild_cursor SET position = 7", [])
            .unwrap();
        assert_eq!(
            (absent, rebuild_cursor(&conn).map_err(|e| e.0)),
            (
                Err("the graph file holds no rebuild state".to_string()),
                Ok(7)
            )
        );
    }

    /// Spec 124, THE CURSOR AND ITS SEEDING: a file already carrying `caught_up` keeps its cursor,
    /// and its verdict is lost only beside a log shorter than the prefix the cursor claims - a head
    /// below the cursor, or no head beside a cursor above 0, as when the log is replaced under the
    /// file - and loses nothing beside a head at or above it.
    #[test]
    fn a_cursored_file_s_seeding_is_lost_only_beside_a_head_below_its_cursor() {
        let cases: [(Position, Option<Position>, bool); 6] = [
            (2, Some(3), false),
            (2, Some(2), false),
            (2, Some(1), true),
            (2, None, true),
            (0, None, false),
            (0, Some(5), false),
        ];
        for (cursor, head, lost) in cases {
            let (_dir, path, p) = seeded_graph();
            crate::test_support::move_the_cursor(Path::new(&path), cursor);
            assert_eq!(
                (
                    seeded_unread(&p, head),
                    crate::test_support::caught_up_at(Path::new(&path)),
                ),
                (Seeding::Ran { lost }, Some(cursor)),
                "a cursor at {cursor} beside a head at {head:?}"
            );
        }
    }

    /// Spec 124, A LOG SWITCHED UNDER A STANDING GRAPH: a cursor recorded beside the log the
    /// projector folds is kept, and one recorded beside another log - the embedded one at another
    /// path, or a server's - or beside none is switched under its file.
    #[test]
    fn a_cursor_recorded_beside_another_log_or_none_is_switched_and_beside_the_selected_one_kept() {
        const SELECTED: &str = "local:/p/.rigger/events.db";
        let cases = [
            (Some(SELECTED), false),
            (Some("local:/q/.rigger/events.db"), true),
            (Some("shared:5cfaa9387d2839e1"), true),
            (None, true),
        ];
        for (recorded, switched) in cases {
            assert_eq!(
                log_switched(recorded, SELECTED),
                switched,
                "a cursor recorded beside {recorded:?}"
            );
        }
    }

    /// Spec 124, A LOG SWITCHED UNDER A STANDING GRAPH: a cursored file's seeding reads the
    /// identity of the log its cursor was recorded beside, and no log row: beside the log this
    /// projector folds it loses nothing, and beside another one, or beside none - a cursor whose
    /// table predates the identity - it is lost, never an error, the cursor standing where it
    /// stood for the rebuild to seed anew.
    #[test]
    fn a_cursored_file_s_seeding_is_lost_beside_another_log_or_none() {
        const SELECTED: &str = "local:/p/.rigger/events.db";
        /// What a case records beside the cursor of the graph file at its path.
        type Recording = fn(&Path);
        let cases: [(&str, Recording, bool); 3] = [
            (
                "the log this projector folds",
                |db| crate::test_support::record_the_cursor_beside(db, SELECTED),
                false,
            ),
            (
                "a server's log",
                |db| crate::test_support::record_the_cursor_beside(db, "shared:5cfaa9387d2839e1"),
                true,
            ),
            ("no log", crate::test_support::drop_the_cursor_s_log, true),
        ];
        for (case, recorded, lost) in cases {
            let (_dir, path, seeded) = seeded_graph();
            drop(seeded);
            crate::test_support::move_the_cursor(Path::new(&path), 2);
            recorded(Path::new(&path));
            let p = Projector::open(&path, "test")
                .unwrap()
                .with_store_identity(SELECTED.to_string());
            assert_eq!(
                (
                    seeded_unread(&p, Some(3)),
                    crate::test_support::caught_up_at(Path::new(&path)),
                ),
                (Seeding::Ran { lost }, Some(2)),
                "a cursor recorded beside {case}"
            );
        }
    }

    /// Whether `p`'s applied ledger records each position from 1 through `last`, in order
    /// ([`applied`]).
    fn applied_through(p: &Projector, last: u64) -> Vec<bool> {
        (1..=last).map(|at| applied(p, at)).collect()
    }

    /// Whether `p`'s applied ledger records `position`.
    fn applied(p: &Projector, position: u64) -> bool {
        p.conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM applied WHERE position = ?1)",
                [position as i64],
                |r| r.get(0),
            )
            .unwrap()
    }

    /// A cold rebuild yields the graph `rigger reset --runs` leaves, never a larger one: at its swap
    /// it applies the run-closure prune to a copy of the shadow, deriving the drop set and the edge
    /// boundary from the run attribution it gathered in the same ordered pass - a closed run's and
    /// a pre-boundary decision or finding is dropped, a lesson and an id the active run reuses are
    /// kept - and a rebuild interrupted after its first batch resumes to the very same graph.
    #[test]
    fn a_rebuild_applies_the_run_closure_prune_before_its_swap_even_when_resumed() {
        use rigger_domain::run::{superseded_edge_boundary, superseded_graph_nodes};
        let log = [
            decision_at("d-pre", "p.rs", 1),
            run_started_at("r1", 2),
            decision_at("d-dead", "a.rs", 3),
            event_at(
                TYPE_REVIEW_FINDING,
                br#"{"id":"f-dead","about":["a.rs"]}"#,
                4,
            ),
            decision_at("shared", "s.rs", 5),
            event_at(TYPE_LESSON_LEARNED, br#"{"id":"l1","about":["a.rs"]}"#, 6),
            run_started_at("r2", 7),
            decision_at("d-live", "b.rs", 8),
            decision_at("shared", "s.rs", 9),
        ];
        let live = Projector::open(":memory:", "test").unwrap();
        crate::test_support::folds(&live, &log);
        live.prune(
            &superseded_graph_nodes(&log),
            superseded_edge_boundary(&log),
        )
        .unwrap();
        let expected = vec!["d-live".to_string(), "l1".to_string(), "shared".to_string()];
        assert_eq!(provenance(&live), expected, "what reset --runs leaves");

        let dir = tempfile::tempdir().unwrap();
        let whole = dir.path().join("whole.db");
        let whole = whole.to_str().unwrap();
        drop(Projector::open(whole, "test").unwrap());
        rebuild_in_batches(whole, &log, 2, &mut Vec::new()).unwrap();

        let resumed = dir.path().join("resumed.db");
        let resumed = resumed.to_str().unwrap();
        drop(Projector::open(resumed, "test").unwrap());
        let interrupted = Projector::open(resumed, "test").unwrap().rebuild(
            &locked(resumed),
            true,
            &mut |after, sink| {
                let first = stream_past(&log, after, 2, sink);
                first.and(Err(Error(FIRST_BATCH_INTERRUPTED.to_string())))
            },
            &mut |_| {},
        );
        assert!(interrupted.is_err(), "the first rebuild is interrupted");
        rebuild_in_batches(resumed, &log, 2, &mut Vec::new()).unwrap();

        assert_eq!(
            (
                provenance(&Projector::open(whole, "test").unwrap()),
                provenance(&Projector::open(resumed, "test").unwrap()),
                crate::test_support::whole_json(&Projector::open(whole, "test").unwrap()),
            ),
            (
                expected.clone(),
                expected,
                crate::test_support::whole_json(&live),
            ),
            "the rebuilt graph, whole or resumed, holds what reset --runs leaves, node and edge"
        );
    }

    /// The decision, finding and lesson node ids `p` holds, sorted.
    fn provenance(p: &Projector) -> Vec<String> {
        let mut ids: Vec<String> = p
            .whole()
            .unwrap()
            .nodes
            .into_iter()
            .filter(|n| [KIND_DECISION, KIND_FINDING, KIND_LESSON].contains(&n.kind.as_str()))
            .map(|n| n.id)
            .collect();
        ids.sort();
        ids
    }

    /// The graph `rigger reset --runs` leaves over `log`, every node and edge as JSON: the whole
    /// log folded, then pruned by the drop set and the edge boundary the one rule derives from it.
    fn reset_runs_leaves(log: &[Event]) -> String {
        use rigger_domain::run::{superseded_edge_boundary, superseded_graph_nodes};
        let live = Projector::open(":memory:", "test").unwrap();
        crate::test_support::folds(&live, log);
        live.prune(&superseded_graph_nodes(log), superseded_edge_boundary(log))
            .unwrap();
        crate::test_support::whole_json(&live)
    }

    /// Tear the rebuild state of the shadow file at `shadow`: drop the run attribution its rebuild
    /// gathered, leaving its cursor.
    fn drop_the_gathered_attribution(shadow: &str) {
        Connection::open(shadow)
            .unwrap()
            .execute_batch("DROP TABLE rebuild_run_closure;")
            .unwrap();
    }

    /// Whether the graph file at `path` holds the rebuild's cursor and the run attribution the
    /// rebuild gathered.
    fn rebuild_state(path: &str) -> (bool, bool) {
        let conn = Connection::open(path).unwrap();
        (
            has_table(&conn, "rebuild_cursor").unwrap(),
            has_table(&conn, "rebuild_run_closure").unwrap(),
        )
    }

    /// A rebuild whose swap stops after it put the pruned copy in place - the owed mark it must
    /// drop is a directory it cannot remove - leaves its shadow unpruned, holding its cursor and the
    /// run attribution it gathered, and no copy behind. When the log gains a window meanwhile, the
    /// rebuild that reruns as `rigger setup` reruns it - whose seeding reads nothing and owes no
    /// lost fold, the copy the swap put in place carrying `caught_up` beside its rebuild state -
    /// still resumes that shadow, because its swap never
    /// finished, without refolding a committed batch - it reads only the window's positions - and
    /// prunes a fresh copy of it from the whole gathered attribution: a closed run's decision the
    /// active run records again governs both files, as its two recordings say; a decision the
    /// active run records is kept; a run started meanwhile closes the run that was active. Each is the graph a cold rebuild of the whole log yields and the
    /// graph `rigger reset --runs` leaves over it, reporting the cold rebuild's counts, and the
    /// finished rebuild drops its state with its cursor.
    #[test]
    fn a_rebuild_interrupted_in_its_swap_resumes_its_unpruned_shadow_to_the_cold_graph() {
        use rigger_domain::run::{superseded_edge_boundary, superseded_graph_nodes};
        let log = [
            run_started_at("r1", 1),
            decision_at("d-dead", "a.rs", 2),
            run_started_at("r2", 3),
            decision_at("d-live", "b.rs", 4),
        ];
        let windows = [
            (
                vec![decision_at("d-dead", "c.rs", 5)],
                vec!["d-dead", "d-live"],
                vec![4, 5],
            ),
            (
                vec![decision_at("d-window", "c.rs", 5)],
                vec!["d-live", "d-window"],
                vec![4, 5],
            ),
            (
                vec![run_started_at("r3", 5), decision_at("d-r3", "c.rs", 6)],
                vec!["d-r3"],
                vec![4, 6],
            ),
        ];
        for (window, kept, reads) in windows {
            let dir = tempfile::tempdir().unwrap();
            let fresh = |name: &str| {
                let path = dir.path().join(name).to_str().unwrap().to_string();
                drop(Projector::open(&path, "test").unwrap());
                path
            };
            let path = fresh("graph.db");
            let stopped = rebuild_stopped_in_its_swap(&path, &log);
            let through_the_swap = (
                rebuild_state(&shadow_of(&path)),
                rebuild_state(&path),
                Path::new(&pruned_copy(&path)).exists(),
            );

            let whole: Vec<Event> = log.iter().cloned().chain(window.iter().cloned()).collect();
            let mut reads_from = Vec::new();
            let mut read = Vec::new();
            let owed = owed_once_seeded_against(&Projector::open(&path, "test").unwrap(), &whole);
            let resumed = Projector::open(&path, "test")
                .unwrap()
                .rebuild(
                    &locked(&path),
                    crate::test_support::setup_owes(&owed),
                    &mut |after, sink| {
                        reads_from.push(after);
                        stream_past(&whole, after, 10, &mut |events, head| {
                            read.extend(crate::test_support::positions_of(events));
                            sink(events, head)
                        })
                    },
                    &mut |_| {},
                )
                .unwrap();
            let p = Projector::open(&path, "test").unwrap();
            let cold = fresh("cold.db");
            let cold_report = rebuild_in_batches(&cold, &whole, 10, &mut Vec::new()).unwrap();
            let reset_runs = {
                let folded = Projector::open(":memory:", "test").unwrap();
                crate::test_support::folds(&folded, &whole);
                folded
                    .prune(
                        &superseded_graph_nodes(&whole),
                        superseded_edge_boundary(&whole),
                    )
                    .unwrap()
            };
            assert_eq!(
                (
                    stopped,
                    through_the_swap,
                    owed,
                    reads_from,
                    read,
                    provenance(&p),
                    rebuild_state(&path),
                    crate::test_support::whole_json(&p),
                    crate::test_support::whole_json(&p),
                    resumed,
                    resumed.map(|r| r.pruned),
                ),
                (
                    refused_at_its_owed_mark(&path),
                    ((true, true), (true, true), false),
                    crate::test_support::NOTHING_OWED_ONCE_SEEDED,
                    reads,
                    crate::test_support::positions_of(&window),
                    crate::test_support::owned_ids(&kept),
                    (false, false),
                    serde_json::to_string(
                        &Projector::open(&cold, "test").unwrap().whole().unwrap()
                    )
                    .unwrap(),
                    reset_runs_leaves(&whole),
                    cold_report,
                    Some(reset_runs),
                ),
                "the rerun resumes the unpruned shadow, reads only the window, and prunes a copy \
                 to the cold rebuild's graph and counts, the graph reset --runs leaves"
            );
        }
        let d_dead_governs = |log: &[Event]| -> Vec<String> {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("graph.db");
            let path = path.to_str().unwrap();
            drop(Projector::open(path, "test").unwrap());
            rebuild_in_batches(path, log, 10, &mut Vec::new()).unwrap();
            live_governs(&Projector::open(path, "test").unwrap())
                .into_iter()
                .filter(|g| g.0 == "d-dead")
                .map(|g| g.1)
                .collect()
        };
        let reused: Vec<Event> = log
            .iter()
            .cloned()
            .chain([decision_at("d-dead", "c.rs", 5)])
            .collect();
        assert_eq!(
            d_dead_governs(&reused),
            vec!["a.rs".to_string(), "c.rs".to_string()],
            "the id the active run records again governs what both of its recordings govern"
        );
    }

    /// A rebuild of the graph file at `path` from `log` whose swap stops once its pruned copy is in
    /// place - the owed mark it must then drop is a directory it cannot remove - and what it
    /// answered.
    fn rebuild_stopped_in_its_swap(path: &str, log: &[Event]) -> Result<Option<Rebuilt>, String> {
        let mark = owed_mark(path).unwrap();
        std::fs::create_dir(&mark).unwrap();
        let stopped = rebuild_in_batches(path, log, 10, &mut Vec::new()).map_err(|e| e.to_string());
        std::fs::remove_dir(&mark).unwrap();
        stopped
    }

    /// What [`rebuild_stopped_in_its_swap`] answers for the graph file at `path`: the refusal to
    /// remove the owed mark that stands as a directory.
    fn refused_at_its_owed_mark(path: &str) -> Result<Option<Rebuilt>, String> {
        Err(crate::test_support::directory_refusal(
            owed_mark(path).unwrap().display(),
        ))
    }

    /// A stale copy an interrupted swap left - a file that is not even a graph - is never read: the
    /// next rebuild removes it before it derives its own, reaches the graph a rebuild without it
    /// reaches, and leaves no copy behind.
    #[test]
    fn a_stale_pruned_copy_is_removed_by_the_next_rebuild_and_never_read() {
        let log = [
            run_started_at("r1", 1),
            decision_at("d-dead", "a.rs", 2),
            run_started_at("r2", 3),
            decision_at("d-live", "b.rs", 4),
        ];
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();
        drop(Projector::open(path, "test").unwrap());
        let copy = pruned_copy(path);
        std::fs::write(&copy, b"left by a swap that stopped").unwrap();

        let rebuilt = rebuild_in_batches(path, &log, 10, &mut Vec::new());
        let p = Projector::open(path, "test").unwrap();
        assert_eq!(
            (
                rebuilt.map_err(|e| e.to_string()),
                Path::new(&copy).exists(),
                crate::test_support::whole_json(&p),
            ),
            (
                Ok(Some(Rebuilt {
                    pruned: PruneStats {
                        nodes: 1,
                        superseded_edges: 0
                    }
                })),
                false,
                reset_runs_leaves(&log),
            ),
            "the stale copy is removed, never read, and the rebuild reaches the graph reset --runs \
             leaves"
        );
    }

    /// A rebuild interrupted in its tail leaves the live file owing only that tail: the file it
    /// swapped in carries the `caught_up` its shadow opened with beside its rebuild state, so the
    /// seeding `rigger setup` runs reads nothing and a position past the swapped-in cursor is the
    /// tail's debt, never a lost fold - setup finishes exactly the tail, reading only its
    /// positions, reports the counts stamped with the swap and sets the cursor at the tail's last
    /// position. On a file an older binary swapped in, carrying no `caught_up`, a tail position
    /// older than the catch-up horizon the ledger misses below one an emit folded past the tail is
    /// still the tail's debt, and the swapped-in cursor's own position missing from the ledger,
    /// older than the horizon, is a lost fold.
    #[test]
    fn a_rebuild_interrupted_in_its_tail_is_finished_by_setup_reading_only_the_tail() {
        let log = [
            run_started_at("r1", 1),
            decision_at("d-dead", "a.rs", 2),
            run_started_at("r2", 3),
            decision_at("d-live", "b.rs", 4),
        ];
        let tail = [
            decision_at("d-tail", "c.rs", 5),
            decision_at("d-more", "d.rs", 6),
        ];
        let whole: Vec<Event> = log.iter().cloned().chain(tail.iter().cloned()).collect();
        // A rebuild of a fresh graph file whose swap takes `log` and whose tail is interrupted:
        // the file it leaves.
        let stopped_in_its_tail = |dir: &tempfile::TempDir| {
            let path = dir.path().join("graph.db").to_str().unwrap().to_string();
            drop(Projector::open(&path, "test").unwrap());
            let mut reads = 0;
            let stopped = Projector::open(&path, "test")
                .unwrap()
                .rebuild(
                    &locked(&path),
                    true,
                    &mut |after, sink| {
                        reads += 1;
                        match reads {
                            1 => stream_past(&log, after, 10, sink),
                            _ => Err(Error(TAIL_INTERRUPTED.to_string())),
                        }
                    },
                    &mut |_| {},
                )
                .map_err(|e| e.to_string());
            (path, stopped)
        };
        let dir = tempfile::tempdir().unwrap();
        let (path, stopped) = stopped_in_its_tail(&dir);
        let path = path.as_str();
        let swapped_in = crate::test_support::caught_up_at(Path::new(path));
        let owed_tail_only =
            owed_once_seeded_against(&Projector::open(path, "test").unwrap(), &whole);

        let mut read = Vec::new();
        let owed = crate::test_support::setup_owes(&owed_tail_only);
        let finished = Projector::open(path, "test")
            .unwrap()
            .rebuild(
                &locked(path),
                owed,
                &mut |after, sink| {
                    stream_past(&whole, after, 10, &mut |events, head| {
                        read.extend(crate::test_support::positions_of(events));
                        sink(events, head)
                    })
                },
                &mut |_| {},
            )
            .unwrap();
        let p = Projector::open(path, "test").unwrap();
        assert_eq!(
            (
                stopped,
                swapped_in,
                owed_tail_only,
                read,
                finished,
                owed_once_seeded_against(&p, &whole),
                crate::test_support::caught_up_at(Path::new(path)),
                rebuild_state(path),
            ),
            (
                Err(crate::test_support::graph_error(TAIL_INTERRUPTED)),
                Some(0),
                crate::test_support::NOTHING_OWED_ONCE_SEEDED,
                vec![5, 6],
                Some(Rebuilt {
                    pruned: PruneStats {
                        nodes: 1,
                        superseded_edges: 0
                    }
                }),
                crate::test_support::NOTHING_OWED_ONCE_SEEDED,
                Some(6),
                (false, false),
            ),
            "the tail is the swap's debt, setup folds exactly it, reports the swap's prune and \
             sets the cursor at the tail's last position"
        );

        let dir = tempfile::tempdir().unwrap();
        let (path, _) = stopped_in_its_tail(&dir);
        crate::test_support::drop_the_cursor(Path::new(&path));
        let p = Projector::open(&path, "test").unwrap();
        crate::test_support::folds(&p, &whole[5..]);
        let backdated = crate::test_support::backdated(&whole);
        let owed_below_a_later_fold = owed_once_seeded_against(&p, &backdated);
        p.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM applied WHERE position = 4", [])
            .unwrap();
        assert_eq!(
            (
                owed_below_a_later_fold,
                owed_once_seeded_against(&p, &backdated)
            ),
            (
                crate::test_support::NOTHING_OWED_ONCE_SEEDED,
                crate::test_support::LOST_FOLD_ONCE_SEEDED
            ),
            "an old tail position missing below a later fold is the tail's debt, and the swapped-in \
             cursor's own position missing from the ledger is a lost fold"
        );
    }

    /// A shadow holding a rebuild's cursor without the run attribution it gathered cannot
    /// re-derive the prune from the whole of it, so it is never resumed: the rebuild discards it
    /// and folds the log again from its start, to the graph `rigger reset --runs` leaves.
    #[test]
    fn a_shadow_holding_its_cursor_without_its_gathered_attribution_is_rebuilt_from_the_start() {
        let log = [
            run_started_at("r1", 1),
            decision_at("d-dead", "a.rs", 2),
            run_started_at("r2", 3),
            decision_at("d-live", "b.rs", 4),
        ];
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();
        drop(Projector::open(path, "test").unwrap());
        let interrupted = rebuild_interrupted_after_its_first_batch(path, &log);
        let shadow = shadow_of(path);
        drop_the_gathered_attribution(&shadow);
        // While another holds the rebuild, the torn shadow is refused to a rebuild, which reads no
        // log and leaves it byte for byte; its holder discards it.
        let torn = std::fs::read(&shadow).unwrap();
        let holder = locked(path);
        let mut refused_reads = Vec::new();
        let refused =
            rebuild_in_batches(path, &log, 2, &mut refused_reads).map_err(|e| e.to_string());
        let untouched = std::fs::read(&shadow).unwrap() == torn;
        drop(holder);

        let mut reads_from = Vec::new();
        rebuild_in_batches(path, &log, 2, &mut reads_from).unwrap();
        let p = Projector::open(path, "test").unwrap();
        assert_eq!(
            (refused, refused_reads, untouched),
            (
                Err(crate::test_support::graph_error(REBUILD_IN_PROGRESS)),
                Vec::<u64>::new(),
                true
            ),
            "a torn shadow another holds the rebuild for is refused and left as it stands"
        );
        assert_eq!(
            (
                interrupted.map_err(|e| e.to_string()),
                reads_from,
                provenance(&p),
                crate::test_support::whole_json(&p),
            ),
            (
                Err(crate::test_support::graph_error(FIRST_BATCH_INTERRUPTED)),
                vec![0, 4],
                vec!["d-live".to_string()],
                reset_runs_leaves(&log),
            ),
            "the shadow without its attribution is folded again from the start and pruned by the \
             whole log's attribution"
        );
    }

    /// A rebuild reports the prune it made - the dead-run nodes and superseded edges the one prune
    /// body removed from its pruned copy, the counts `rigger reset --runs` reports over the whole
    /// log's graph - and a rebuild resumed past that prune reports the very same counts, never
    /// zero: one whose swap stopped, and one interrupted in its tail. One resumed over a window the
    /// log gained reports the one prune of the whole log, the cold rebuild's counts.
    #[test]
    fn a_rebuild_reports_its_prune_and_a_rebuild_resumed_past_it_reports_the_same_counts() {
        use rigger_domain::run::{superseded_edge_boundary, superseded_graph_nodes};
        let at = |mut e: Event, secs: u64| {
            e.valid_from = std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs);
            e
        };
        let decision = |id: &str, file: &str, supersedes: &str, position: u64| {
            let payload = serde_json::json!({
                "id": id, "summary": "x", "governs": [file], "supersedes": supersedes
            });
            at(
                event_at(
                    TYPE_DECISION_MADE,
                    &serde_json::to_vec(&payload).unwrap(),
                    position,
                ),
                position,
            )
        };
        let log = [
            at(run_started_at("r1", 1), 1),
            decision("shared", "f.rs", "", 2),
            decision("d-sup", "f.rs", "shared", 3),
            at(
                event_at(
                    TYPE_REVIEW_FINDING,
                    br#"{"id":"f-dead","about":["f.rs"]}"#,
                    4,
                ),
                4,
            ),
            at(run_started_at("r2", 5), 5),
            decision("shared", "g.rs", "", 6),
        ];
        let reset_runs = {
            let live = Projector::open(":memory:", "test").unwrap();
            crate::test_support::folds(&live, &log);
            live.prune(
                &superseded_graph_nodes(&log),
                superseded_edge_boundary(&log),
            )
            .unwrap()
        };
        let pruned = PruneStats {
            nodes: 2,
            superseded_edges: 1,
        };
        let report = Some(Rebuilt { pruned });
        let dir = tempfile::tempdir().unwrap();
        let fresh = |name: &str| {
            let path = dir.path().join(name).to_str().unwrap().to_string();
            drop(Projector::open(&path, "test").unwrap());
            path
        };

        let whole = fresh("whole.db");
        let rebuilt = rebuild_in_batches(&whole, &log, 10, &mut Vec::new()).unwrap();

        let stopped = fresh("stopped.db");
        let swap_stopped = rebuild_stopped_in_its_swap(&stopped, &log);
        let resumed_past_the_swap =
            rebuild_in_batches(&stopped, &log, 10, &mut Vec::new()).unwrap();

        // A run started while the swap stood stopped closes the run that was active: the rerun
        // reports its one prune of the whole log, the cold rebuild's counts - `shared` dropped with
        // the retired edge it owns, which no prune reclaimed first - never the sum of two prunes.
        let window: Vec<Event> = log
            .iter()
            .cloned()
            .chain([at(run_started_at("r3", 7), 7)])
            .collect();
        let windowed = fresh("windowed.db");
        let window_stopped = rebuild_stopped_in_its_swap(&windowed, &log);
        let resumed_over_the_window =
            rebuild_in_batches(&windowed, &window, 10, &mut Vec::new()).unwrap();
        let cold_over_the_window =
            rebuild_in_batches(&fresh("cold.db"), &window, 10, &mut Vec::new()).unwrap();

        let tail = fresh("tail.db");
        let mut reads = 0;
        let tail_stopped = Projector::open(&tail, "test")
            .unwrap()
            .rebuild(
                &locked(&tail),
                true,
                &mut |after, sink| {
                    reads += 1;
                    if reads == 2 {
                        return Err(Error(TAIL_INTERRUPTED.to_string()));
                    }
                    stream_past(&log, after, 10, sink)
                },
                &mut |_| {},
            )
            .map_err(|e| e.to_string());
        let resumed_in_the_tail = Projector::open(&tail, "test")
            .unwrap()
            .rebuild(
                &locked(&tail),
                false,
                &mut |after, sink| stream_past(&log, after, 10, sink),
                &mut |_| {},
            )
            .unwrap();

        assert_eq!(
            (
                reset_runs,
                rebuilt,
                swap_stopped,
                resumed_past_the_swap,
                tail_stopped,
                resumed_in_the_tail,
                window_stopped,
                resumed_over_the_window,
                cold_over_the_window,
            ),
            (
                pruned,
                report,
                refused_at_its_owed_mark(&stopped),
                report,
                Err(crate::test_support::graph_error(TAIL_INTERRUPTED)),
                report,
                refused_at_its_owed_mark(&windowed),
                Some(Rebuilt {
                    pruned: PruneStats {
                        nodes: 3,
                        superseded_edges: 0
                    }
                }),
                Some(Rebuilt {
                    pruned: PruneStats {
                        nodes: 3,
                        superseded_edges: 0
                    }
                }),
            ),
            "the rebuild reports what reset --runs prunes, a rebuild resumed past its swap or in its \
             tail reports the same, and one resumed over a window reports the cold rebuild's one \
             prune"
        );
    }

    /// A log that never started a run has no closed run, so nothing in it is dead: its rebuild
    /// prunes nothing and says so, keeping every decision, finding and lesson node and every edge
    /// its fold holds.
    #[test]
    fn a_rebuild_of_a_log_without_a_run_boundary_prunes_nothing() {
        let log = [
            decision_at("d1", "a.rs", 1),
            event_at(TYPE_REVIEW_FINDING, br#"{"id":"f1","about":["a.rs"]}"#, 2),
            event_at(TYPE_LESSON_LEARNED, br#"{"id":"l1","about":["a.rs"]}"#, 3),
            decision_at("d4", "b.rs", 4),
        ];
        let folded = Projector::open(":memory:", "test").unwrap();
        crate::test_support::folds(&folded, &log);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();
        drop(Projector::open(path, "test").unwrap());

        let rebuilt = rebuild_in_batches(path, &log, 10, &mut Vec::new()).unwrap();
        let p = Projector::open(path, "test").unwrap();
        assert_eq!(
            (
                rebuilt,
                provenance(&p),
                crate::test_support::whole_json(&p),
            ),
            (
                Some(Rebuilt::default()),
                vec![
                    "d1".to_string(),
                    "d4".to_string(),
                    "f1".to_string(),
                    "l1".to_string()
                ],
                crate::test_support::whole_json(&folded),
            ),
            "nothing is dead before a run starts: the rebuild prunes nothing and keeps the fold whole"
        );
    }

    /// A `DecisionMade` `id` governing `file` at log position `position`.
    fn decision_at(id: &str, file: &str, position: u64) -> Event {
        let payload = serde_json::json!({
            "id": id, "summary": "x", "governs": [file], "supersedes": ""
        });
        let mut e = Event::new(TYPE_DECISION_MADE, serde_json::to_vec(&payload).unwrap());
        e.position = position;
        e
    }

    /// A batch another writer's write lock holds off a current `graph.db` past its ceiling writes
    /// no mark (spec 124, THE APPENDER): the fold answers the write-lock text, no process holds
    /// the debt, and the file itself carries it in its ledger, which misses the lost position
    /// below the next batch's - so it outlives the process, and once the hole is older than the
    /// catch-up horizon the seeding `rigger setup` runs finds the prefix violated and its rebuild
    /// pays it. Its events name no stream, so the text carries no suffix (CAUSE TEXTS).
    #[test]
    fn a_fold_lost_past_its_ceiling_writes_no_mark_and_is_owed_by_the_ledger_alone() {
        use crate::contextgraph::Fold;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();
        let mark = crate::test_support::owed_mark_in(dir.path());
        let log = decisions_through(3);
        let p = Projector::open(path, "test")
            .unwrap()
            .with_lock_ceiling(Duration::ZERO);
        crate::test_support::folds(&p, &log[..1]);
        impatient(&p);
        let folded = crate::test_support::with_graph_locked(Path::new(path), || {
            crate::test_support::fold_of(&p, &log[1..])
        });
        assert_eq!(
            (folded, p.owed_because().unwrap(), mark.exists()),
            (
                Fold::NotFolded(crate::test_support::graph_error(&held_past_a_zero_ceiling())),
                Vec::<&str>::new(),
                false,
            ),
            "the fold answers the write-lock text past its ceiling, and nothing records the loss"
        );
        crate::test_support::folds(&p, &log[2..]);
        drop(p);
        the_ledger_owes_the_lost_fold_and_a_rebuild_pays_it(
            path,
            &crate::test_support::backdated(&log),
            2,
        );
    }

    /// `events` appended to the run stream of the project `test` in the log beside the graph file
    /// at `path` ([`logged`]), read back as the copies their appender folds: each with the position
    /// and the stream the log gave it.
    fn appended_copies(path: &str, events: &[Event]) -> Vec<Event> {
        logged(&*the_unscoped_log_beside(path)().unwrap(), events);
        let page = the_log_beside(path)()
            .unwrap()
            .read_all_page(0, &crate::eventstore::Filter::default(), 4096)
            .unwrap();
        page[page.len() - events.len()..].to_vec()
    }

    /// Whether the appender of `copies` answered past `ceiling`: the clock has run longer than it
    /// since the store stamped the lowest of them, the instant the appender's ceiling counts from
    /// (spec 124, THE APPENDER).
    fn answered_past(copies: &[Event], ceiling: Duration) -> bool {
        SystemTime::now()
            .duration_since(copies[0].recorded_at)
            .unwrap()
            > ceiling
    }

    /// The appender's copies a case builds beside the graph file at its path.
    type Copies = fn(&str) -> Vec<Event>;

    /// A decision at position 1 appended to the log beside the graph file at `path`, read back as
    /// its appender's copy ([`appended_copies`]).
    fn one_decision(path: &str) -> Vec<Event> {
        appended_copies(path, &decisions_at([1]))
    }

    /// The copy of the last of decisions at 1 through one past [`FOLD_BATCH`] appended to the log
    /// beside the graph file at `path` ([`appended_copies`]): a first call advances a whole batch
    /// and stops short of it.
    fn beyond_one_batch(path: &str) -> Vec<Event> {
        appended_copies(path, &decisions_at(1..=FOLD_BATCH as u64 + 1))[FOLD_BATCH..].to_vec()
    }

    /// A copy of the run stream at position 1 the log beside the graph file never received, so
    /// every call meets the log's head below it.
    fn beyond_the_log_s_head(_: &str) -> Vec<Event> {
        let mut copy = decision_at("d1", "a.rs", 1);
        copy.stream = rigger_domain::run::STREAM.to_string();
        vec![copy]
    }

    /// The copy of a decision at position 1 ([`one_decision`]) of a graph file at `path` holding
    /// the cursor a swapped-in rebuild's tail is owed past ([`a_standing_tail`]).
    fn behind_a_standing_tail(path: &str) -> Vec<Event> {
        a_standing_tail(path);
        one_decision(path)
    }

    /// The appender's one copy: the event `at` builds at position 1 - a decision whose payload the
    /// judge rejects - appended to the log
    /// beside the graph file at `path` and read back ([`appended_copies`]).
    fn own_copy(path: &str, at: fn(u64) -> Event) -> Vec<Event> {
        appended_copies(path, &[at(1)])
    }

    /// How a case's appender folds its copies through its projector, given the graph file's path.
    type Folding = fn(&str, &Projector, &[Event]) -> crate::contextgraph::Fold;

    /// The appender's fold of its copies through its projector, nothing else acting on the file.
    fn alone(_: &str, p: &Projector, copies: &[Event]) -> crate::contextgraph::Fold {
        crate::test_support::fold_of(p, copies)
    }

    /// The cause a fold answers past a zero ceiling (`with_lock_ceiling(Duration::ZERO)`) while
    /// another writer holds the write lock ([`lock_held_past`](crate::test_support::lock_held_past)).
    fn held_past_a_zero_ceiling() -> String {
        crate::test_support::lock_held_past("0 s")
    }

    /// Spec 124, CAUSE TEXTS: a ceiling reads in whole minutes at or above one minute and in whole
    /// seconds below it, each rounded down.
    #[test]
    fn a_ceiling_reads_in_whole_minutes_from_one_minute_and_in_whole_seconds_below() {
        let reads = [
            0, 999, 1_000, 59_999, 60_000, 61_000, 119_999, 120_000, 600_000,
        ]
        .map(|millis| held_for(Duration::from_millis(millis)));
        assert_eq!(
            reads,
            ["0 s", "0 s", "1 s", "59 s", "1 min", "1 min", "1 min", "2 min", "10 min"],
            "whole seconds below a minute, whole minutes from one"
        );
    }

    /// Spec 124, THE APPENDER: the write-lock fixture holds another writer's lock past the busy
    /// timeout a projector's connection waits on it, the interval its appender waits between
    /// calls, and inside the appender's ceiling.
    #[test]
    fn the_write_lock_fixture_holds_past_the_busy_timeout_and_inside_the_ceiling() {
        let (_dir, path) = crate::test_support::temp_graph_db();
        let p = Projector::open(&path, "test").unwrap();
        let busy: u64 = p
            .conn
            .lock()
            .unwrap()
            .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
            .unwrap();
        let held = crate::test_support::HELD_PAST_THE_BUSY_TIMEOUT;
        assert_eq!(
            (held > Duration::from_millis(busy), held < FOLD_LOCK_CEILING),
            (true, true),
            "the fixture holds the lock past the busy timeout and inside the ceiling"
        );
    }

    /// Spec 124, THE APPENDER and CAUSE TEXTS: an appender another writer's write lock holds off
    /// past its ceiling returns not-folded with the write-lock text, its own positions unfolded and
    /// no mark written, the text ending with its path's suffix: after its call, on a file carrying
    /// `caught_up` through a projector handed the log, the call-run suffix; in place of its direct
    /// fold's retried open, on a file without the cursor or through a projector handed no log, the
    /// direct suffix; and for copies not all of the run stream - one naming no stream beside one of
    /// it - folded directly whatever the file carries, none.
    #[test]
    fn an_appender_held_off_past_its_ceiling_answers_the_write_lock_text_and_marks_nothing() {
        use crate::contextgraph::Fold;
        use crate::test_support::{graph_error, lost_by_calls, lost_fold, SETUP_PAYS};
        let reason = held_past_a_zero_ceiling();
        let held = graph_error(&reason);
        // Each case: what stands in the file, whether the projector is handed the log, whether both
        // the appender's copies keep the stream the log gave them - else the first names none - and
        // what its fold answers.
        let cases: [(&str, &[Stand], bool, bool, String); 4] = [
            ("a call", &[], true, true, lost_by_calls(&reason)),
            (
                "a direct fold on a file without the cursor",
                &[without_a_cursor],
                true,
                true,
                lost_fold(&held, SETUP_PAYS),
            ),
            (
                "a direct fold through a projector handed no log",
                &[],
                false,
                true,
                lost_fold(&held, SETUP_PAYS),
            ),
            (
                "a direct fold of copies one of which names no stream",
                &[],
                true,
                false,
                held.clone(),
            ),
        ];
        for (case, stands, handed, streamed, text) in cases {
            let (_dir, path, p) = seeded_graph();
            for stand in stands {
                stand(&path);
            }
            let mut copies = appended_copies(&path, &decisions_at([1, 2]));
            if !streamed {
                copies[0].stream.clear();
            }
            let p = match handed {
                true => p.with_log(the_log_beside(&path)),
                false => p,
            }
            .with_lock_ceiling(Duration::ZERO);
            impatient(&p);
            let folded = crate::test_support::with_graph_locked(Path::new(&path), || {
                crate::test_support::fold_of(&p, &copies)
            });
            assert_eq!(
                (
                    folded,
                    applied_through(&p, 2),
                    crate::test_support::mark_stands_beside(Path::new(&path))
                ),
                (Fold::NotFolded(text), vec![false, false], false),
                "{case}: the write-lock text with its path's suffix, nothing folded and no mark"
            );
        }
    }

    /// Spec 124, THE APPENDER and CAUSE TEXTS: past its ceiling an appender still short of the
    /// cursor returns not-folded with the cause of its last call and the call-run suffix: after a
    /// call that advanced short of its position, or one that met the log's head below it, the
    /// catch-up text; after one the file's rebuild state held inert, the unfinished-rebuild text.
    /// None writes the mark.
    #[test]
    fn past_its_ceiling_an_appender_answers_the_cause_of_its_last_call() {
        use crate::contextgraph::Fold;
        use crate::test_support::lost_by_calls;
        let short = lost_by_calls(&crate::test_support::catch_up_short_after("0 s"));
        // Each case: the appender's copies, given the graph file's path, and what its fold answers.
        type Case = (&'static str, Copies, String);
        let cases: [Case; 3] = [
            (
                "a call that advanced short of it",
                beyond_one_batch,
                short.clone(),
            ),
            (
                "a call that met the log's head below it",
                beyond_the_log_s_head,
                short,
            ),
            (
                "a call the file's rebuild state held inert",
                behind_a_standing_tail,
                lost_by_calls(crate::test_support::UNFINISHED_REBUILD),
            ),
        ];
        for (case, copies, text) in cases {
            let (_dir, path, p) = seeded_graph();
            let copies = copies(&path);
            let p = p
                .with_log(the_log_beside(&path))
                .with_lock_ceiling(Duration::ZERO);
            assert_eq!(
                (
                    crate::test_support::fold_of(&p, &copies),
                    applied(&p, copies[0].position),
                    crate::test_support::mark_stands_beside(Path::new(&path)),
                ),
                (Fold::NotFolded(text), false, false),
                "{case}: the last call's cause with the call-run suffix, nothing folded, no mark"
            );
        }
    }

    /// Spec 124, THE APPENDER and CAUSE TEXTS: an appender returns at once, inside its ceiling,
    /// when a call fails at a position other than its own, with the call's error and
    /// the call-run suffix; when a call meets an older fold rule another writer stamped, with the
    /// refusal of a file that owes its rebuild; when a call meets a cursor recorded beside another
    /// log, with the refusal naming the lost fold, folding nothing, before the success test, which
    /// would read that log's prefix as its own; and when its own position is passed over by its
    /// call, or was
    /// recorded passed over before its direct fold on a file without the cursor, with the cause it
    /// was passed over for and no suffix. None writes the mark.
    #[test]
    fn an_appender_returns_at_once_on_a_foreign_failure_an_owed_rebuild_or_its_position_passed_over(
    ) {
        use crate::contextgraph::Fold;
        use crate::test_support::{graph_error, lost_by_calls};
        // Each case: the appender's copies and its projector, given the graph file's path, its
        // log and the projector handed that log; and what its fold answers, given the copies.
        type Handed = fn(&str, &CountedLog, Projector) -> (Vec<Event>, Projector);
        type Answer = fn(&[Event]) -> Fold;
        // The cause the judge passed the appender's one copy over for, with no suffix.
        let passed_over: Answer = |copies| {
            let (_, cause) = crate::test_support::rejected_row(&copies[0]);
            Fold::NotFolded(graph_error(&cause))
        };
        // The refusal of a file whose cursor records another log, naming the fold it lost.
        let lost_fold_refusal: Answer =
            |_| Fold::NotFolded(graph_error(crate::contextgraph::REBUILD_OWED_FOR_LOST_FOLD));
        let cases: [(&str, Handed, Answer); 6] = [
            (
                "a foreign record failing its call",
                |path, _, p| {
                    let copies = appended_copies(
                        path,
                        &[
                            decision_at(crate::test_support::D_BOOM, "b.rs", 1),
                            decision_at("d2", "a.rs", 2),
                        ],
                    );
                    crate::test_support::the_disk_gives_out_at_d_boom(Path::new(path));
                    (copies[1..].to_vec(), p)
                },
                |_| Fold::NotFolded(lost_by_calls(crate::test_support::DISK_GAVE_OUT)),
            ),
            (
                "an older fold rule stamped under the call",
                |path, log, p| {
                    let copies = one_decision(path);
                    let standing = path.to_string();
                    log.after_call(0, move |_| folded_under_an_older_rule(&standing));
                    (copies, p)
                },
                |_| crate::test_support::owed_refusal(),
            ),
            (
                "a cursor recorded beside another log below its own position",
                |path, _, p| {
                    let copies = one_decision(path);
                    beside_another_log(path);
                    (copies, p)
                },
                lost_fold_refusal,
            ),
            (
                "a cursor recorded beside another log at its own position",
                |path, _, p| {
                    let copies = one_decision(path);
                    crate::test_support::move_the_cursor(Path::new(path), 1);
                    beside_another_log(path);
                    (copies, p)
                },
                lost_fold_refusal,
            ),
            (
                "its own position passed over by its call",
                |path, _, p| (own_copy(path, crate::test_support::poisoned_decision_at), p),
                passed_over,
            ),
            (
                "its own position recorded passed over before its direct fold",
                |path, _, p| {
                    let copies = own_copy(path, crate::test_support::poisoned_decision_at);
                    p.catch_up().unwrap();
                    without_a_cursor(path);
                    (copies, p)
                },
                passed_over,
            ),
        ];
        let ceiling = Duration::from_secs(30);
        for (case, handed, answer) in cases {
            let (_dir, path, p, log) = catching_up();
            let (copies, p) = handed(&path, &log, p.with_lock_ceiling(ceiling));
            assert_eq!(
                (
                    crate::test_support::fold_of(&p, &copies),
                    answered_past(&copies, ceiling),
                    crate::test_support::mark_stands_beside(Path::new(&path)),
                ),
                (answer(&copies), false, false),
                "{case}: answered at once, inside the ceiling, writing no mark"
            );
        }
    }

    /// Spec 124, THE APPENDER: an appender meeting its file's rebuild state waits it out under its
    /// ceiling, unless its own process holds that file's rebuild lock - the tell
    /// [`Projector::with_rebuild_lock`] records - when it returns at once, since only that
    /// process's tail folds the event; either way it answers the unfinished-rebuild text and the
    /// call-run suffix, writing no mark. Another file's lock, or none, records nothing.
    #[test]
    fn an_appender_meeting_rebuild_state_returns_at_once_only_holding_its_file_s_rebuild_lock() {
        use crate::contextgraph::Fold;
        use crate::test_support::{lost_by_calls, UNFINISHED_REBUILD};
        // Each case: the graph file whose rebuild lock the projector is handed, given its own
        // file's path, or none; its ceiling; and whether it answers past that ceiling.
        type Locked = fn(&str) -> Option<String>;
        let cases: [(&str, Locked, Duration, bool); 3] = [
            (
                "its own file's lock",
                |path| Some(path.to_string()),
                Duration::from_secs(30),
                false,
            ),
            (
                "another file's lock",
                |path| Some(format!("{path}.other")),
                Duration::from_secs(1),
                true,
            ),
            ("no lock", |_| None, Duration::from_secs(1), true),
        ];
        for (case, locked, ceiling, past) in cases {
            let (_dir, path, p) = seeded_graph();
            let copies = behind_a_standing_tail(&path);
            let held = locked(&path).map(|file| Projector::lock_rebuild(&file).unwrap());
            let p = p
                .with_log(the_log_beside(&path))
                .with_lock_ceiling(ceiling)
                .with_rebuild_lock(held.as_ref());
            impatient(&p);
            assert_eq!(
                (
                    crate::test_support::fold_of(&p, &copies),
                    answered_past(&copies, ceiling),
                    crate::test_support::mark_stands_beside(Path::new(&path)),
                ),
                (
                    Fold::NotFolded(lost_by_calls(UNFINISHED_REBUILD)),
                    past,
                    false,
                ),
                "{case}: the unfinished-rebuild text, past the ceiling only when waited out"
            );
        }
    }

    /// Spec 124, THE APPENDER: an appender's own positions fold - on a file carrying `caught_up`
    /// through a projector handed the log, by its call, the cursor moved past them, even with its
    /// ceiling already passed, since the success test reads before the ceiling; on a file a
    /// seeding gives the cursor while the direct fold's first open waits on the seeding's write
    /// lock, by its call too, since that open reads the cursor again under the lock; on a file
    /// whose cursor another writer dropped under that call, and on one without it, by its direct
    /// fold - and no mark is written.
    #[test]
    fn an_appender_s_positions_fold_by_its_call_or_directly_where_the_file_has_no_cursor() {
        use crate::contextgraph::Fold;
        // Each case: what stands in the file before the fold, what stands in it the moment the
        // call reads the log, the appender's ceiling, how it folds, and where the cursor stands
        // after the fold.
        type Stands = &'static [Stand];
        type Case = (&'static str, Stands, Stands, Duration, Folding, Option<u64>);
        let cases: [Case; 5] = [
            ("its call", &[], &[], FOLD_LOCK_CEILING, alone, Some(1)),
            (
                "its call with its ceiling already passed",
                &[],
                &[],
                Duration::ZERO,
                alone,
                Some(1),
            ),
            (
                "its call once a seeding lands under its direct fold's first open",
                &[without_a_cursor],
                &[],
                FOLD_LOCK_CEILING,
                // The seeding takes the write lock before the fold starts and commits a fifth of
                // the busy timeout later, inside the wait of the direct fold's first open.
                |path, p, copies| {
                    crate::test_support::under_a_seedings_lock(
                        || Projector::open(path, "test").unwrap(),
                        &[],
                        p.busy_interval().unwrap() / 5,
                        || crate::test_support::fold_of(p, copies),
                    )
                },
                Some(1),
            ),
            (
                "its direct fold once the cursor is dropped under its call",
                &[],
                &[without_a_cursor],
                FOLD_LOCK_CEILING,
                alone,
                None,
            ),
            (
                "its direct fold on a file without the cursor",
                &[without_a_cursor],
                &[],
                FOLD_LOCK_CEILING,
                alone,
                None,
            ),
        ];
        for (case, before, under, ceiling, folding, cursor) in cases {
            let (_dir, path, p, log) = catching_up();
            let p = p.with_lock_ceiling(ceiling);
            for stand in before {
                stand(&path);
            }
            let standing = path.clone();
            log.after_call(0, move |_| {
                for stand in under {
                    stand(&standing);
                }
            });
            let copies = one_decision(&path);
            assert_eq!(
                (
                    folding(&path, &p, &copies),
                    applied(&p, 1),
                    crate::test_support::caught_up_at(Path::new(&path)),
                    crate::test_support::mark_stands_beside(Path::new(&path)),
                ),
                (Fold::Folded, true, cursor, false),
                "{case}: folded, the cursor where it stands, no mark"
            );
        }
    }

    /// Spec 124, THE APPENDER: an appender's ceiling counts from the stamp of its lowest own
    /// position, wherever among its events that position comes and whatever the others' stamps;
    /// an appender with no position to wait for is past it.
    #[test]
    fn an_appender_s_ceiling_counts_from_the_stamp_of_its_lowest_own_position() {
        let ceiling = Duration::from_secs(60);
        let (_dir, path) = crate::test_support::temp_graph_db();
        let p = Projector::open(&path, "test")
            .unwrap()
            .with_lock_ceiling(ceiling);
        let (now, young, old) = (SystemTime::now(), Duration::ZERO, ceiling * 2);
        // Each case: how long before now each own position was stamped, in the order the
        // appender's events come in, and whether the appender is past its ceiling.
        type Ages<'a> = &'a [(u64, Duration)];
        let cases: [(&str, Ages<'_>, bool); 4] = [
            (
                "its lowest position, between the others, older than the ceiling",
                &[(2, young), (1, old), (3, young)],
                true,
            ),
            (
                "only its first position older than it",
                &[(2, old), (1, young), (3, young)],
                false,
            ),
            (
                "only its last position older than it",
                &[(2, young), (1, young), (3, old)],
                false,
            ),
            ("no position to wait for", &[], true),
        ];
        for (case, ages, past) in cases {
            let events: Vec<Event> = ages
                .iter()
                .flat_map(|&(at, age)| {
                    crate::test_support::restamped(&decisions_at([at]), |_| now - age)
                })
                .collect();
            assert_eq!(
                p.past_ceiling(&events),
                past,
                "{case}: whether the appender is past its ceiling"
            );
        }
    }

    /// Spec 124, THE APPENDER: an appender with no own position to wait for ends its calls
    /// folded after the first, since nothing of its stands above the cursor.
    #[test]
    fn an_appender_with_no_own_position_ends_its_calls_folded_after_the_first() {
        let (_dir, _path, p, _log) = catching_up();
        assert_eq!(
            p.fold_by_calls(&[], Duration::ZERO).map_err(|e| e.0),
            Ok(true),
            "folded by its calls, with no position to wait for"
        );
    }

    /// Spec 124, THE APPENDER: an appender waits between two attempts only after one that did
    /// not advance, its connection's lock released through the wait. A call that advanced is
    /// followed by the next at once; one that met the log's head below the appender's position,
    /// one another writer's write lock skipped and one the file's rebuild state held inert are
    /// each followed by one busy interval of the connection, as is
    /// a direct fold's open that met that write lock past the busy timeout - and another user of
    /// the projector asking for the connection halfway through the appender's last wait takes it
    /// at once.
    #[test]
    fn an_appender_waits_a_busy_interval_with_its_connection_released_after_each_attempt_that_did_not_advance(
    ) {
        use crate::contextgraph::Fold;
        use crate::test_support::lost_by_calls;
        /// What an appender of the copies `copies` builds answers, folded by `folding` under the
        /// ceiling `ceiling`; the whole busy intervals of its connection its fold took; and
        /// whether another user of the projector, asking for the connection halfway through the
        /// last of the `intervals` the fold is to take - in each case a wait of the appender's -
        /// was kept waiting a quarter of one.
        fn waited(
            copies: Copies,
            folding: Folding,
            ceiling: Duration,
            intervals: u32,
        ) -> (Fold, u32, bool) {
            let (_dir, path, p, _log) = catching_up();
            let copies = copies(&path);
            let p = p.with_lock_ceiling(ceiling);
            let busy = p.busy_interval().unwrap();
            std::thread::scope(|scope| {
                let asker = scope.spawn(|| {
                    if intervals == 0 {
                        return false;
                    }
                    std::thread::sleep(busy * (2 * intervals - 1) / 2);
                    let asked = std::time::Instant::now();
                    drop(p.conn.lock().unwrap());
                    asked.elapsed() >= busy / 4
                });
                let started = std::time::Instant::now();
                let folded = folding(&path, &p, &copies);
                let took = started.elapsed().as_millis() / busy.as_millis();
                (folded, u32::try_from(took).unwrap(), asker.join().unwrap())
            })
        }
        let locked: Folding = |path, p, copies| {
            crate::test_support::with_graph_locked_for(Path::new(path), || {
                crate::test_support::fold_of(p, copies)
            })
        };
        let without_one: Copies = |path| {
            without_a_cursor(path);
            one_decision(path)
        };
        // Each case: the appender's copies, given the graph file's path; how it folds them; its
        // ceiling; what its fold answers; and the whole busy intervals its fold takes - one for
        // each wait, and one for each attempt that met the write lock, since such an attempt
        // waits the connection's busy timeout before it gives up.
        type Case = (&'static str, Copies, Folding, Duration, Fold, u32);
        let cases: [Case; 5] = [
            (
                "a call that advanced short of it",
                beyond_one_batch,
                alone,
                FOLD_LOCK_CEILING,
                Fold::Folded,
                0,
            ),
            (
                "a call that met the log's head below it",
                beyond_the_log_s_head,
                alone,
                Duration::from_secs(1),
                Fold::NotFolded(lost_by_calls(&crate::test_support::catch_up_short_after(
                    "1 s",
                ))),
                1,
            ),
            (
                "a call another writer's write lock skipped",
                one_decision,
                locked,
                FOLD_LOCK_CEILING,
                Fold::Folded,
                2,
            ),
            (
                "a call the file's rebuild state held inert",
                behind_a_standing_tail,
                alone,
                Duration::from_secs(1),
                Fold::NotFolded(lost_by_calls(crate::test_support::UNFINISHED_REBUILD)),
                1,
            ),
            (
                "a direct fold's open past another writer's write lock",
                without_one,
                locked,
                FOLD_LOCK_CEILING,
                Fold::Folded,
                2,
            ),
        ];
        // The cases run side by side, each on its own file, since each waits whole busy timeouts.
        let observed: Vec<_> = std::thread::scope(|scope| {
            let running = cases.map(|(case, copies, folding, ceiling, answer, intervals)| {
                let run = scope.spawn(move || waited(copies, folding, ceiling, intervals));
                (case, answer, intervals, run)
            });
            running
                .into_iter()
                .map(|(case, answer, intervals, run)| {
                    (case, answer, intervals, run.join().unwrap())
                })
                .collect()
        });
        for (case, answer, intervals, observed) in observed {
            assert_eq!(
                observed,
                (answer, intervals, false),
                "{case}: the answer, the whole busy intervals the fold took, and the connection's \
                 lock free inside the last wait"
            );
        }
    }

    /// An event the log holds that no fold ever reached - its process died between the append
    /// and the fold, so nothing ran to record anything, while a later event folded - is owed by
    /// the ledger all the same once it is older than the catch-up horizon, and the rebuild pays
    /// it.
    #[test]
    fn an_event_appended_but_never_folded_is_owed_by_the_ledger() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();
        let log = decisions_through(3);
        let p = Projector::open(path, "test").unwrap();
        crate::test_support::folds(&p, &[log[0].clone(), log[2].clone()]);
        drop(p);
        the_ledger_owes_the_lost_fold_and_a_rebuild_pays_it(
            path,
            &crate::test_support::backdated(&log),
            2,
        );
    }

    /// A file beside the owed mark whose seeding finds an old hole below its highest applied
    /// position owes the lost fold by the seeding's verdict alone, the mark naming no cause, and
    /// seeds no cursor. One at an older fold rule is not seeded, whatever its ledger misses: its
    /// seeding finds the lost fold beside the older rule it records only for an old miss.
    #[test]
    fn the_lost_fold_is_the_seeding_s_verdict_and_the_owed_mark_names_no_cause() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let log = crate::test_support::backdated(&decisions_through(3));
        let p = Projector::open(path.to_str().unwrap(), "test").unwrap();
        crate::test_support::folds(&p, &[log[0].clone(), log[2].clone()]);
        crate::test_support::mark_owed_in(dir.path());
        assert_eq!(
            (
                owed_once_seeded_against(&p, &log),
                crate::test_support::caught_up_at(&path)
            ),
            (crate::test_support::LOST_FOLD_ONCE_SEEDED, None),
            "the seeding names the lost fold, and the mark is no cause"
        );
        let at_an_older_rule = |folded: &[Event]| {
            let (_dir, file) = crate::test_support::temp_graph_db();
            let p = Projector::open(&file, "test").unwrap();
            crate::test_support::folds(&p, folded);
            p.conn
                .lock()
                .unwrap()
                .pragma_update(None, "user_version", PROJECTION_VERSION - 1)
                .unwrap();
            (
                owed_once_seeded_against(&p, &log),
                crate::test_support::caught_up_at(Path::new(&file)),
            )
        };
        assert_eq!(
            [
                at_an_older_rule(&[log[0].clone(), log[2].clone()]),
                at_an_older_rule(&log),
            ],
            [
                ((vec![OWED_OLDER_RULE], Seeding::Ran { lost: true }), None),
                ((vec![OWED_OLDER_RULE], Seeding::Ran { lost: false }), None),
            ],
            "a file at an older rule is not seeded, and owes the lost fold beside it only for an \
             old miss"
        );
    }

    /// A graph file owes its rebuild for the older fold rule it records and for nothing else it
    /// carries: the owed mark beside it is no cause, so a current file owes nothing with or without
    /// the mark, and one at an older rule owes that rule alone.
    #[test]
    fn a_graph_file_owes_its_rebuild_for_the_older_rule_it_records_and_never_for_its_mark() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();
        let p = Projector::open(path, "test").unwrap();
        crate::test_support::apply_decision(&p, 1, "d1", "x", &["a.rs"], "");
        assert_eq!(p.owed_because().unwrap(), Vec::<&str>::new());
        crate::test_support::mark_owed_in(dir.path());
        assert_eq!(p.owed_because().unwrap(), Vec::<&str>::new());
        p.conn
            .lock()
            .unwrap()
            .pragma_update(None, "user_version", PROJECTION_VERSION - 1)
            .unwrap();
        assert_eq!(p.owed_because().unwrap(), vec![OWED_OLDER_RULE]);
    }

    /// The causes `rigger setup` names for a graph file that records the causes `recorded` and
    /// whose seeding answered `seeding` ([`causes_once_seeded`]) are `named`: the recorded ones,
    /// then the lost fold when the seeding ran and found the prefix it folded violated.
    fn setup_names(recorded: Vec<&'static str>, seeding: Seeding, named: Vec<&'static str>) {
        assert_eq!(
            causes_once_seeded(recorded, &seeding),
            named,
            "setup names the recorded causes, then the seeding's lost fold"
        );
    }

    crate::test_cases! {
        /// A contended seeding issues no verdict: the recorded cause alone.
        a_skipped_seeding_adds_no_cause_to_the_recorded_ones:
            setup_names(vec![OWED_OLDER_RULE], Seeding::Skipped, vec![OWED_OLDER_RULE]);
        /// A seeding that found nothing lost, beside a recorded cause: that cause alone.
        a_seeding_that_found_nothing_lost_adds_no_cause_to_the_recorded_ones:
            setup_names(
                vec![OWED_OLDER_RULE],
                Seeding::Ran { lost: false },
                vec![OWED_OLDER_RULE],
            );
        /// A seeding that found nothing lost, beside no recorded cause: no cause.
        a_seeding_that_found_nothing_lost_beside_no_recorded_cause_names_none:
            setup_names(vec![], Seeding::Ran { lost: false }, vec![]);
        /// A seeding that found the prefix violated, beside no recorded cause: the lost fold alone.
        a_seeding_that_found_the_prefix_violated_names_the_lost_fold:
            setup_names(vec![], Seeding::Ran { lost: true }, vec![OWED_LOST_FOLD]);
        /// A seeding that found the prefix violated, beside a recorded cause: that cause first, then
        /// the lost fold.
        a_seeding_that_found_the_prefix_violated_names_the_lost_fold_after_the_recorded_cause:
            setup_names(
                vec![OWED_OLDER_RULE],
                Seeding::Ran { lost: true },
                vec![OWED_OLDER_RULE, OWED_LOST_FOLD],
            );
    }

    /// The shared-node rule (spec 116) is fold rule 2, the path-addressed node (spec 119) rule 3,
    /// a record's reference read as the id it names rule 4, a lesson's or finding's `file` read as
    /// one more location rule 5 and a community membership held only by code rule 6: a file that
    /// folded under rule 1 took a shared node's kind and line from the last definition folded, one
    /// that folded under rule 2 holds a nested definition at its flat id, one that folded under
    /// rule 3 holds a record citing lines of a file ABOUT an artifact at the citation rather than
    /// the file, one that folded under rule 4 holds a record naming its file by `file` with no
    /// ABOUT edge to it and one that folded under rule 5 keeps a membership on a path only a record
    /// still names, so each owes its rebuild, and one at the current rule owes none.
    #[test]
    fn a_graph_file_folded_at_an_older_projection_version_owes_its_rebuild_and_one_at_the_current_owes_none(
    ) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let p = Projector::open(path.to_str().unwrap(), "test").unwrap();
        crate::test_support::apply_decision(&p, 1, "d1", "x", &["a.rs"], "");
        let owed_at = |version: i64| {
            p.conn
                .lock()
                .unwrap()
                .pragma_update(None, "user_version", version)
                .unwrap();
            p.owed_because().unwrap()
        };
        for version in 1..PROJECTION_VERSION {
            assert_eq!(owed_at(version), vec![OWED_OLDER_RULE], "rule {version}");
        }
        assert_eq!(owed_at(PROJECTION_VERSION), Vec::<&str>::new());
    }

    /// The owed mark an older binary's lost fold left belongs to the graph file beside it: once
    /// that file is gone the mark is dropped, and a file made in its place owes nothing - and while
    /// the file stands its mark is kept, no cause of the file's rebuild.
    #[test]
    fn the_owed_mark_of_a_graph_file_that_is_gone_is_dropped_and_one_that_stands_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();
        let mark = crate::test_support::mark_owed_in(dir.path());
        Projector::forget_orphaned_mark(path).unwrap();
        assert!(
            !mark.exists(),
            "the mark of a graph file that is gone is dropped"
        );
        assert_eq!(
            Projector::open(path, "test")
                .unwrap()
                .owed_because()
                .unwrap(),
            Vec::<&str>::new(),
            "a file made in its place owes nothing"
        );
        crate::test_support::mark_owed_in(dir.path());
        Projector::forget_orphaned_mark(path).unwrap();
        assert_eq!(
            (
                mark.exists(),
                Projector::open(path, "test")
                    .unwrap()
                    .owed_because()
                    .unwrap()
            ),
            (true, Vec::<&str>::new()),
            "the mark of a graph file that stands is kept, and is no cause"
        );
        Projector::forget_orphaned_mark(":memory:").unwrap();
    }

    /// An owed mark beside the graph file at a path that cannot be removed - a directory stands at
    /// its path ([`unremovable_mark_beside`](crate::test_support::unremovable_mark_beside)) - fails
    /// its forgetting by `forget` naming the mark, and stands.
    fn a_mark_that_cannot_be_removed_fails_naming_it(forget: fn(&str) -> Result<(), Error>) {
        let (_dir, path) = crate::test_support::temp_graph_db();
        let mark = crate::test_support::unremovable_mark_beside(Path::new(&path));
        assert_eq!(
            (forget(&path).map_err(|e| e.to_string()), mark.is_dir()),
            (
                Err(crate::test_support::directory_refusal(mark.display())),
                true
            ),
            "the mark that cannot be removed is named, and stands"
        );
    }

    crate::test_cases! {
        /// The graph file is gone: its orphaned mark is forgotten
        /// ([`Projector::forget_orphaned_mark`]).
        an_orphaned_mark_that_cannot_be_removed_fails_naming_it:
            a_mark_that_cannot_be_removed_fails_naming_it(Projector::forget_orphaned_mark);
        /// The graph file carries its `caught_up` cursor ([`seeded_at`]): its mark is forgotten as
        /// a cursored file's ([`Projector::forget_cursored_mark`]).
        a_cursored_graph_files_mark_that_cannot_be_removed_fails_naming_it:
            a_mark_that_cannot_be_removed_fails_naming_it(|path| {
                seeded_at(path).forget_cursored_mark()
            });
    }

    /// A stale pruned copy is forgotten - removed, answering that it was - whenever no one holds
    /// the rebuild lock, whatever stands beside it; while another holds the lock the copy may be
    /// a rebuild's own and is kept, answering so; with no copy nothing is removed and no lock file
    /// is made. The shadow beside it is never removed or rewritten here.
    #[test]
    fn a_stale_pruned_copy_is_forgotten_unless_a_rebuild_holds_the_lock() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();
        let copy = dir.path().join("graph.db.pruned");
        let shadow = dir.path().join("graph.db.rebuild");
        let stale = || std::fs::write(&copy, b"left by a swap that stopped").unwrap();
        let forgotten = || (Projector::forget_stale_copy(path).unwrap(), copy.exists());

        let nothing = (forgotten(), dir.path().join("graph.db.lock").exists());
        stale();
        let alone = forgotten();
        stale();
        let rebuilding = locked(path);
        let held = forgotten();
        std::fs::write(&shadow, b"the shadow the rebuild folds").unwrap();
        let held_through_its_fold = forgotten();
        drop(rebuilding);
        let released = forgotten();
        assert_eq!(
            (
                nothing,
                alone,
                held,
                held_through_its_fold,
                released,
                std::fs::read(&shadow).unwrap()
            ),
            (
                ((StaleCopy::Absent, false), false),
                (StaleCopy::Removed, false),
                (StaleCopy::InUse, true),
                (StaleCopy::InUse, true),
                (StaleCopy::Removed, false),
                b"the shadow the rebuild folds".to_vec()
            ),
            "a copy stood beside a free lock is removed, one beside a held lock is kept, \
             and the shadow stays as it was"
        );
    }

    /// A rebuild is left unfinished over a graph file that stands with the shadow a rebuild resumes
    /// beside it, or that holds the cursor whose tail a rebuild finishes - and over no other: not
    /// one that stands with neither, nor once the cursor is dropped, and never where no graph file
    /// stands, shadow or not, which is answered without making one. The shadow is never opened: it
    /// is not a database here, and stands byte for byte.
    #[test]
    fn a_rebuild_is_unfinished_over_a_standing_shadow_or_a_held_cursor_and_never_without_a_graph_file(
    ) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();
        let shadow = dir.path().join("graph.db.rebuild");
        let left = b"left by a rebuild that stopped";
        let held = locked(path);
        let lock_only = dir_snapshot(dir.path(), "");
        // What is answered, and what stands beside the lock once it is.
        let unfinished = || {
            (
                Projector::rebuild_unfinished(&held).map_err(|e| e.to_string()),
                dir_snapshot(dir.path(), ""),
            )
        };

        let no_graph_file = unfinished();
        std::fs::write(&shadow, left).unwrap();
        let a_shadow_beside_no_graph_file = unfinished();
        std::fs::remove_file(&shadow).unwrap();
        drop(Projector::open(path, "test").unwrap());
        let neither = unfinished().0;
        std::fs::write(&shadow, left).unwrap();
        let a_standing_shadow = (unfinished().0, std::fs::read(&shadow).unwrap());
        std::fs::remove_file(&shadow).unwrap();
        let graph = Connection::open(path).unwrap();
        graph.execute_batch(REBUILD_STATE).unwrap();
        let the_cursor = unfinished().0;
        graph.execute_batch(DROP_REBUILD_STATE).unwrap();
        let the_cursor_dropped = unfinished().0;
        assert_eq!(
            (
                no_graph_file,
                a_shadow_beside_no_graph_file,
                neither,
                a_standing_shadow,
                the_cursor,
                the_cursor_dropped
            ),
            (
                (Ok(false), lock_only.clone()),
                (
                    Ok(false),
                    vec![
                        ("graph.db.lock".to_string(), Some(vec![])),
                        ("graph.db.rebuild".to_string(), Some(left.to_vec())),
                    ]
                ),
                Ok(false),
                (Ok(true), left.to_vec()),
                Ok(true),
                Ok(false)
            ),
            "only a graph file that stands beside a shadow, or holds the cursor, has a rebuild \
             left unfinished, and asking makes no graph file; beside the lock alone stood \
             {lock_only:?}"
        );
    }

    /// Whether paying a graph file's rebuild starts a fresh shadow (spec 124) is answered by what
    /// stands. With no shadow, a cause owed - the caller's, or the file's own older fold rule over
    /// what it folded - starts one, a swapped-in tail standing or not, and no cause starts none:
    /// the owed mark an older binary's lost fold left beside the file is none. A standing shadow is
    /// resumed, cause or not, when it holds the run attribution it gathered under the current fold
    /// rule from the log the rebuild folds - its cursor recorded beside that log, or carrying no
    /// cursor, as a binary before the cursor left it - and replaced otherwise: one folded under an
    /// older or a newer rule, one without that attribution, one whose rebuild state was never
    /// opened, or one whose cursor was recorded beside another log or beside none (spec 124, A LOG
    /// SWITCHED UNDER A STANDING GRAPH). Asking makes no shadow and leaves a standing one byte for
    /// byte.
    #[test]
    fn a_rebuild_starts_a_fresh_shadow_for_a_cause_beside_none_or_a_shadow_it_cannot_resume() {
        // A rebuild of the graph file at `path` stopped past its first batch: the resumable
        // shadow it leaves.
        fn a_resumable_shadow(path: &str) {
            rebuild_interrupted_after_its_first_batch(path, &decisions_through(4)).unwrap_err();
        }
        // That shadow as a binary folding under the fold rule `rule` leaves it.
        fn a_shadow_folded_under(path: &str, rule: i64) {
            a_resumable_shadow(path);
            crate::test_support::stamp_user_version(Path::new(&shadow_of(path)), rule)
        }
        // The graph file at `path` with nothing standing beside it.
        fn nothing_stands(_: &str) {}
        // Each case: what stands, the `owed` it is asked with, and whether the pay starts fresh.
        let cases: [(&str, Stand, bool, bool); 15] = [
            ("nothing owed", nothing_stands, false, false),
            ("a cause the caller owes", nothing_stands, true, true),
            (
                "an older fold rule the file records",
                folded_under_an_older_rule,
                false,
                true,
            ),
            (
                "an owed mark the file carries, which is no cause",
                carrying_an_owed_mark,
                false,
                false,
            ),
            (
                "a tail no cause is owed beside",
                a_standing_tail,
                false,
                false,
            ),
            (
                "a tail beside a cause the caller owes",
                a_standing_tail,
                true,
                true,
            ),
            ("a resumable shadow", a_resumable_shadow, false, false),
            (
                "a resumable shadow beside a cause the caller owes",
                a_resumable_shadow,
                true,
                false,
            ),
            (
                "a shadow an older-rule binary left",
                |path| a_shadow_folded_under(path, PROJECTION_VERSION - 1),
                false,
                true,
            ),
            (
                "a shadow a newer-rule binary left",
                |path| a_shadow_folded_under(path, PROJECTION_VERSION + 1),
                false,
                true,
            ),
            (
                "a shadow carrying no cursor, as a binary before the cursor left it",
                |path| {
                    a_resumable_shadow(path);
                    crate::test_support::drop_the_cursor(Path::new(&shadow_of(path)))
                },
                false,
                false,
            ),
            (
                "a shadow whose cursor was recorded beside another log",
                |path| {
                    a_resumable_shadow(path);
                    crate::test_support::record_the_cursor_beside(
                        Path::new(&shadow_of(path)),
                        "local:/elsewhere/.rigger/events.db",
                    )
                },
                false,
                true,
            ),
            (
                "a shadow whose cursor was recorded beside no log",
                |path| {
                    a_resumable_shadow(path);
                    crate::test_support::drop_the_cursor_s_log(Path::new(&shadow_of(path)))
                },
                false,
                true,
            ),
            (
                "a shadow without the run attribution it gathered",
                |path| {
                    a_resumable_shadow(path);
                    drop_the_gathered_attribution(&shadow_of(path))
                },
                false,
                true,
            ),
            (
                "a shadow whose rebuild state was never opened",
                |path| schema(&Connection::open(shadow_of(path)).unwrap(), "test").unwrap(),
                false,
                true,
            ),
        ];
        for (case, stand, owed, fresh) in cases {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("graph.db");
            let path = path.to_str().unwrap();
            drop(Projector::open(path, "test").unwrap());
            stand(path);
            let shadow = || dir_snapshot(dir.path(), &shadow_of("graph.db"));
            let standing = shadow();
            let starts_fresh = Projector::open(path, "test")
                .unwrap()
                .rebuild_starts_fresh(&locked(path), owed)
                .map_err(|e| e.to_string());
            assert_eq!(
                (starts_fresh, shadow()),
                (Ok(fresh), standing),
                "{case}: the pay's fresh-shadow answer, and the shadow left as it stood"
            );
        }
    }

    /// The rebuild lock of a graph file is the OS lock on the zero-byte `<path>.lock` beside it,
    /// made by the first lock that needs it: while one holder has it a second is refused at once
    /// with the one text, finding and leaving only that lock file; once the holder drops it the
    /// next takes it, and the lock file stands - no holder removes it.
    #[test]
    fn the_rebuild_lock_is_refused_to_a_second_holder_with_the_one_text_and_free_once_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();
        let beside = || dir_snapshot(dir.path(), "");
        let found = beside();
        let held = Projector::lock_rebuild(path).unwrap();
        let taken = beside();
        let second = Projector::lock_rebuild(path)
            .map(drop)
            .map_err(|e| e.to_string());
        let refused = beside();
        drop(held);
        let next = Projector::lock_rebuild(path)
            .map(drop)
            .map_err(|e| e.to_string());
        let lock_file = vec![("graph.db.lock".to_string(), Some(vec![]))];
        assert_eq!(
            (found, taken, second, refused, next, beside()),
            (
                vec![],
                lock_file.clone(),
                Err(crate::test_support::graph_error(REBUILD_IN_PROGRESS)),
                lock_file.clone(),
                Ok(()),
                lock_file,
            ),
            "the second holder is refused with the one text and leaves only the lock file, which \
             is free once dropped and stands"
        );
    }

    /// A dropped rebuild lock is free even while another descriptor still shares its lock file's
    /// open description - the copy a child process forked by any thread of this process holds
    /// until it execs, modelled here by a dup of the holder's descriptor: the next lock is taken,
    /// never refused as if a rebuild were still in progress.
    #[test]
    fn a_dropped_rebuild_lock_is_free_while_a_dup_of_its_descriptor_stays_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();
        let held = Projector::lock_rebuild(path).unwrap();
        let forked_copy = held._locked.file().try_clone().unwrap();
        drop(held);
        let next = Projector::lock_rebuild(path)
            .map(drop)
            .map_err(|e| e.to_string());
        drop(forked_copy);
        assert_eq!(
            next,
            Ok(()),
            "the dropped lock is released, whatever other descriptor shares its description"
        );
    }

    /// A rebuild lock file that cannot be opened - a directory stands at its path - fails the lock
    /// naming it, and fails forgetting a stale copy the same way, which then keeps the copy.
    #[test]
    fn a_rebuild_lock_file_that_cannot_be_opened_fails_naming_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();
        let lock = dir.path().join("graph.db.lock");
        let copy = dir.path().join("graph.db.pruned");
        std::fs::create_dir(&lock).unwrap();
        std::fs::write(&copy, b"left by a swap that stopped").unwrap();
        let named = crate::test_support::directory_refusal(lock.display());
        assert_eq!(
            (
                Projector::lock_rebuild(path)
                    .map(drop)
                    .map_err(|e| e.to_string()),
                Projector::forget_stale_copy(path).map_err(|e| e.to_string()),
                copy.exists()
            ),
            (Err(named.clone()), Err(named), true),
            "the lock file that cannot be opened is named, and the copy is kept"
        );
    }

    /// A rebuild lock another process holds refuses this one with the one text while that process
    /// lives - it holds the lock file open - and is free the moment the process is gone, however it
    /// ended: the OS releases it, so no stale lock outlives a rebuild's process.
    #[test]
    fn a_rebuild_lock_whose_holder_process_is_gone_is_free() {
        use crate::test_support::{cleanup, files_open_by, lock_holder};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().canonicalize().unwrap().join("graph.db");
        let path = path.to_str().unwrap();
        let lock = PathBuf::from(format!("{path}.lock"));
        let mut holder = lock_holder(&lock);
        let pid = holder.id();
        let taken = || {
            Projector::lock_rebuild(path)
                .map(drop)
                .map_err(|e| e.to_string())
        };
        let while_it_lives = (files_open_by(pid, &lock), taken());
        cleanup(&mut holder);
        let once_it_is_gone = (files_open_by(pid, &lock), taken());
        assert_eq!(
            (while_it_lives, once_it_is_gone),
            (
                (
                    1,
                    Err(crate::test_support::graph_error(REBUILD_IN_PROGRESS))
                ),
                (0, Ok(()))
            ),
            "the lock is refused while its holder process lives and free once it is gone"
        );
    }

    /// A rebuild started while another holds the rebuild is refused at once with the one text,
    /// whatever the holder is doing - it has taken the lock and written nothing, it folds its
    /// shadow, or its swap has ended and it folds its tail: the refused rebuild reads no log and
    /// leaves every file beside `graph.db`, and `graph.db` itself, exactly as it found them - no
    /// shadow where there was none, no copy, no mark. The holder goes on to the end: `graph.db` is
    /// its graph, the one `rigger reset --runs` leaves, with no rebuild state, shadow or copy.
    #[test]
    fn a_rebuild_started_while_another_holds_the_rebuild_is_refused_and_touches_nothing() {
        let log = [
            run_started_at("r1", 1),
            decision_at("d-dead", "a.rs", 2),
            run_started_at("r2", 3),
            decision_at("d-live", "b.rs", 4),
        ];
        let log = &log;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();
        drop(Projector::open(path, "test").unwrap());
        let shadow = shadow_of(path);
        let files = || dir_snapshot(dir.path(), "");
        // A second rebuild, started now: what it answered, what it read of the log, whether the
        // holder's shadow stood, and whether every file stands exactly as it did.
        let second = || {
            let found = files();
            let mut reads = Vec::new();
            let refused = rebuild_in_batches(path, log, 10, &mut reads).map_err(|e| e.to_string());
            (
                refused,
                reads,
                Path::new(&shadow).exists(),
                files() == found,
            )
        };
        let refused = |shadow_stands| {
            (
                Err(crate::test_support::graph_error(REBUILD_IN_PROGRESS)),
                Vec::<u64>::new(),
                shadow_stands,
                true,
            )
        };

        let held = locked(path);
        let before_its_first_write = second();
        let mut while_it_rebuilds = Vec::new();
        let first = Projector::open(held.path(), "test")
            .unwrap()
            .rebuild(
                &held,
                true,
                &mut |after, sink| {
                    stream_past(log, after, 10, sink)?;
                    while_it_rebuilds.push(second());
                    Ok(())
                },
                &mut |_| {},
            )
            .map_err(|e| e.to_string());
        drop(held);
        let p = Projector::open(path, "test").unwrap();
        assert_eq!(
            (
                before_its_first_write,
                while_it_rebuilds,
                first,
                crate::test_support::whole_json(&p),
                rebuild_state(path),
                Path::new(&shadow).exists(),
                Path::new(&pruned_copy(path)).exists(),
            ),
            (
                refused(false),
                vec![refused(true), refused(false)],
                Ok(Some(Rebuilt {
                    pruned: PruneStats {
                        nodes: 1,
                        superseded_edges: 0
                    }
                })),
                reset_runs_leaves(log),
                (false, false),
                false,
                false,
            ),
            "a second rebuild is refused with the one text in every phase and touches nothing, and \
             graph.db is the first's, with no rebuild state, shadow or copy"
        );
    }

    /// A pruned copy that cannot be removed - a directory stands at its path - is never taken for
    /// gone: with no shadow beside it, forgetting it fails with the directory's own error and the
    /// copy stands, and a rebuild over it fails with that same error rather than deriving its
    /// graph through it.
    #[test]
    fn a_pruned_copy_that_cannot_be_removed_fails_its_forgetting_and_a_rebuild_over_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();
        drop(Projector::open(path, "test").unwrap());
        let copy = dir.path().join("graph.db.pruned");
        std::fs::create_dir(&copy).unwrap();

        let forgotten = Projector::forget_stale_copy(path).map_err(|e| e.to_string());
        let stands = copy.is_dir();
        let rebuilt =
            rebuild_in_batches(path, &[decision_at("d1", "a.rs", 1)], 10, &mut Vec::new())
                .map_err(|e| e.to_string());
        assert_eq!(
            (forgotten, stands, rebuilt, copy.is_dir()),
            (
                Err(crate::test_support::directory_refusal(copy.display())),
                true,
                Err(crate::test_support::directory_refusal(copy.display())),
                true
            ),
            "forgetting the copy and a rebuild over it both fail naming the directory, which stands"
        );
    }

    /// Forgetting a stale copy never reads the shadow beside it: beside a shadow that is not a
    /// database, the copy beside a free lock is removed, and the shadow stands byte for
    /// byte.
    #[test]
    fn forgetting_a_stale_copy_never_reads_the_shadow_beside_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();
        let copy = dir.path().join("graph.db.pruned");
        let shadow = dir.path().join("graph.db.rebuild");
        std::fs::write(&copy, b"left by a swap that stopped").unwrap();
        std::fs::write(&shadow, b"a shadow that is not a database").unwrap();

        assert_eq!(
            (
                Projector::forget_stale_copy(path).map_err(|e| e.to_string()),
                copy.exists(),
                std::fs::read(&shadow).unwrap()
            ),
            (
                Ok(StaleCopy::Removed),
                false,
                b"a shadow that is not a database".to_vec()
            ),
            "the copy is removed and the shadow is never read"
        );
    }

    /// Every LIVE `GOVERNS` edge as `(from, to, source, valid_from)`, read straight from the
    /// table (not through the live `subgraph` filter), so a test can COUNT the rows and prove a
    /// re-assertion collapsed into the one existing live edge rather than accreting a row per fold.
    fn live_governs(p: &Projector) -> Vec<(String, String, i64, i64)> {
        let conn = p.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT from_id, to_id, source, valid_from FROM edges
                 WHERE rel = ?1 AND valid_to IS NULL ORDER BY from_id, to_id",
            )
            .unwrap();
        stmt.query_map(params![REL_GOVERNS], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
    }

    /// Every node as `(id, kind)`.
    type NodeKinds = Vec<(String, String)>;
    /// Every live edge as `(from, rel, to)`.
    type LiveEdges = Vec<(String, String, String)>;

    /// Every node's `(id, kind)` and every LIVE edge's `(from, rel, to)`, read straight from the
    /// tables. Unlike a seeded `subgraph`, this sees the WHOLE graph, so a test can prove a node
    /// kind or edge rel is ABSENT everywhere (a seeded neighborhood could only prove local absence).
    fn all_nodes_edges(p: &Projector) -> (NodeKinds, LiveEdges) {
        let conn = p.conn.lock().unwrap();
        let nodes = {
            let mut s = conn
                .prepare("SELECT id, kind FROM nodes ORDER BY id")
                .unwrap();
            s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        let edges = {
            let mut s = conn
                .prepare(
                    "SELECT from_id, rel, to_id FROM edges
                     WHERE valid_to IS NULL ORDER BY from_id, rel, to_id",
                )
                .unwrap();
            s.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
        };
        (nodes, edges)
    }

    #[test]
    fn re_asserted_governs_folds_to_one_live_edge_with_latest_provenance() {
        // Spec 40 criterion 1 (demonstrated over the surviving GOVERNS edge after the spec 43
        // de-noise dropped the old TOUCHES machinery vehicle): every re-fold of a decision that
        // GOVERNS a file re-asserts `decision --GOVERNS--> file`, and the old bare-insert fold
        // appended a fresh live row per assertion (measured worst case: 45 identical live rows for
        // a single relationship). The upsert-live fold collapses a re-assertion into the ONE
        // existing live edge, bumping its provenance to the LATEST assertion (source) and keeping
        // the EARLIEST valid_from - so N re-assertions yield exactly ONE live edge, while a
        // DIFFERENT decision or a DIFFERENT file still folds its own distinct live edge (dedup
        // removes only EXACT (from, rel, to, tier) duplicates).
        let p = Projector::open(":memory:", "test").unwrap();

        // d1 governs src/f.rs four times (positions 10..=13; valid_from 100..=400s).
        apply_governs_at(&p, 10, "d1", "src/f.rs", 100);
        apply_governs_at(&p, 11, "d1", "src/f.rs", 200);
        apply_governs_at(&p, 12, "d1", "src/f.rs", 300);
        apply_governs_at(&p, 13, "d1", "src/f.rs", 400);

        // A DIFFERENT decision and a DIFFERENT file each fold their own distinct live edge.
        apply_governs_at(&p, 14, "d2", "src/f.rs", 500);
        apply_governs_at(&p, 15, "d1", "src/g.rs", 600);

        let f = to_nanos(UNIX_EPOCH + std::time::Duration::from_secs(100));
        let g = to_nanos(UNIX_EPOCH + std::time::Duration::from_secs(600));
        let b = to_nanos(UNIX_EPOCH + std::time::Duration::from_secs(500));
        assert_eq!(
            live_governs(&p),
            vec![
                // d1->f collapsed from FOUR folds to ONE: source = latest (13), valid_from = earliest.
                ("d1".to_string(), "src/f.rs".to_string(), 13, f),
                // a different FILE is a distinct edge, untouched by the d1->f dedup.
                ("d1".to_string(), "src/g.rs".to_string(), 15, g),
                // a different DECISION is a distinct edge, untouched by the d1->f dedup.
                ("d2".to_string(), "src/f.rs".to_string(), 14, b),
            ],
            "N re-assertions of one relationship collapse to ONE live edge (source=latest, valid_from=earliest); a different decision/file keeps its own distinct live edge"
        );
    }

    #[test]
    fn a_re_assertion_after_invalidation_folds_a_new_live_edge_dedup_keys_on_live_edges_only() {
        // Spec 40 criterion 2: the upsert-live fold keys on LIVE edges ONLY - `add_edge`'s dedup
        // UPDATE carries `AND valid_to IS NULL`. So it collapses a re-assertion into an EXISTING
        // live edge, but NEVER suppresses a legitimate re-assertion of a relationship that has since
        // been INVALIDATED. Drive the GOVERNS supersession path: d1 governs mod.rs (live), d2
        // supersedes d1 (stamping valid_to on d1's GOVERNS edge - invalidated, not deleted), then d1
        // is re-asserted. Because the one prior d1->mod.rs GOVERNS edge is invalidated, the dedup
        // UPDATE matches no live row and the fold INSERTs a NEW live edge, retained beside the dead
        // one. Were the dedup keyed on ALL edges instead of live-only, the re-assertion would be
        // swallowed into the invalidated row and no live edge would exist - the relationship would
        // be silently lost.
        //
        // The structural re-extraction variant of live-only scoping (a superseded
        // CONTAINS/REFERENCES edge re-asserted by a fresh batch) is already exercised by
        // `re_extraction_supersedes_a_files_prior_structural_edges_without_deleting_them`; the
        // GOVERNS supersession path here is the demonstration unique to this criterion. Scope is
        // strictly criterion 2 (live-only scoping): it does NOT own the TOUCHES re-assert fold
        // (criterion 1) nor the rebuild-collapse of pre-existing duplicates (criterion 3).
        let p = Projector::open(":memory:", "test").unwrap();

        // d1 governs mod.rs, then d2 supersedes d1 - stamping valid_to on d1's GOVERNS edge.
        apply_decision(&p, 1, "d1", "v1", &["mod.rs"], "");
        apply_decision(&p, 2, "d2", "v2", &[], "d1");

        // Precondition: exactly ONE d1->mod.rs GOVERNS edge exists and it is now INVALIDATED
        // (valid_to set), so NO live d1->mod.rs GOVERNS edge remains for the dedup to key on.
        let after_supersede: Vec<_> = edges_from(&p, "d1")
            .into_iter()
            .filter(|t| t.1 == REL_GOVERNS && t.0 == "mod.rs")
            .collect();
        assert_eq!(
            after_supersede.len(),
            1,
            "precondition: one d1->mod.rs GOVERNS edge after supersession; got {after_supersede:?}"
        );
        assert!(
            after_supersede[0].2.is_some(),
            "precondition: the supersession invalidated d1's GOVERNS edge (valid_to set); got {after_supersede:?}"
        );
        assert!(
            !p.subgraph(&["mod.rs".to_string()], 2)
                .unwrap()
                .edges
                .iter()
                .any(|e| e.rel == REL_GOVERNS && e.from == "d1"),
            "precondition: the invalidated edge is absent from the live view before the re-assertion"
        );

        // d1 is re-asserted at a later position - a legitimate re-assertion after invalidation.
        apply_decision(&p, 3, "d1", "v1", &["mod.rs"], "");

        // Live-only scoping: the dedup did NOT collapse the re-assertion into the dead row. A NEW
        // live edge is folded and RETAINED beside the invalidated one - exactly ONE historical +
        // ONE live d1->mod.rs GOVERNS edge.
        let after_reassert: Vec<_> = edges_from(&p, "d1")
            .into_iter()
            .filter(|t| t.1 == REL_GOVERNS && t.0 == "mod.rs")
            .collect();
        assert_eq!(
            after_reassert.len(),
            2,
            "the re-assertion folds a NEW row beside the invalidated one, not swallowed into it; got {after_reassert:?}"
        );
        assert_eq!(
            after_reassert.iter().filter(|t| t.2.is_none()).count(),
            1,
            "exactly ONE d1->mod.rs GOVERNS edge is live after the re-assertion; got {after_reassert:?}"
        );
        assert_eq!(
            after_reassert.iter().filter(|t| t.2.is_some()).count(),
            1,
            "the prior invalidated edge is retained (valid_to stamped), never overwritten; got {after_reassert:?}"
        );

        // The live view a grounding consumer reads once again shows d1 governing mod.rs - the
        // re-assertion took effect through a fresh live edge, not the suppressed dead one.
        assert!(
            p.subgraph(&["mod.rs".to_string()], 2)
                .unwrap()
                .edges
                .iter()
                .any(|e| e.rel == REL_GOVERNS && e.from == "d1" && e.to == "mod.rs"),
            "the re-asserted d1->mod.rs GOVERNS edge is LIVE in the projection"
        );
    }

    #[test]
    fn a_rebuild_collapses_existing_duplicate_live_edges_to_one_per_relationship() {
        // Spec 40 criterion 3 (rebuild-dedup / projection idempotency). The graph is a rebuildable
        // projection of the log (spec 29a), so the operational cleanup for the measured 39,340
        // duplicate live edges is a fresh graph REBUILD. This proves it: a log that under the OLD
        // bare `INSERT ... valid_to = NULL` accreted K identical live edges per relationship, folded
        // from scratch into a FRESH projection with the upsert-live `add_edge`, yields exactly ONE
        // live edge per `(from, rel, to, tier)` - with distinct relationships each surviving as
        // their own single live edge. This owns the rebuild-dedup; it leans on (but does not own)
        // the upsert-live fold arm (criterion 1) or the live-only scoping (criterion 2).

        // The canonical log the rebuild re-folds: decision d1 --GOVERNS--> src/f.rs re-asserted 45
        // times (the measured worst case - one fold per re-assertion), interleaved with two
        // DISTINCT relationships (a different decision, a different file) that must each survive the
        // rebuild as their own single live edge. GOVERNS is the surviving content edge the dedup is
        // demonstrated over after the spec 43 de-noise dropped the old TOUCHES machinery vehicle.
        let fold_log = |p: &Projector| {
            for pos in 1..=45u64 {
                apply_governs_at(p, pos, "d1", "src/f.rs", 100 * pos);
            }
            apply_governs_at(p, 46, "d2", "src/f.rs", 5000);
            apply_governs_at(p, 47, "d1", "src/g.rs", 6000);
        };

        // PREMISE - reproduce the dirty on-disk graph the OLD bare-insert left behind. Each of the
        // 45 folds ran exactly this `INSERT ... valid_to = NULL`, so the pre-rebuild graph.db
        // carried 45 live rows for the ONE relationship (identical from/rel/to/tier; only
        // source/valid_from differ). Seed that state directly so the rebuild has a real duplicate
        // pile to collapse, not a hypothetical one.
        let dirty = Projector::open(":memory:", "test").unwrap();
        {
            let conn = dirty.conn.lock().unwrap();
            for pos in 1..=45i64 {
                conn.execute(
                    "INSERT INTO edges (from_id, to_id, rel, valid_from, valid_to, source, project, tier)
                     VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7)",
                    params![
                        "d1",
                        "src/f.rs",
                        REL_GOVERNS,
                        to_nanos(UNIX_EPOCH + std::time::Duration::from_secs((100 * pos) as u64)),
                        pos,
                        "test",
                        TIER_EXTRACTED
                    ],
                )
                .unwrap();
            }
        }
        assert_eq!(
            live_governs(&dirty).len(),
            45,
            "premise: the old bare-insert fold left K=45 identical live rows for one relationship - the duplicates a rebuild must collapse"
        );

        // REBUILD - discard the dirty graph and fold the SAME log from scratch into a FRESH, EMPTY
        // projection. Every relationship collapses to exactly ONE live edge: the 45-fold d1
        // ->src/f.rs to a single row (source = latest position 45, valid_from = earliest), and the
        // two DISTINCT relationships each to their own single live edge.
        let rebuilt = Projector::open(":memory:", "test").unwrap();
        fold_log(&rebuilt);

        let f = to_nanos(UNIX_EPOCH + std::time::Duration::from_secs(100));
        let g = to_nanos(UNIX_EPOCH + std::time::Duration::from_secs(6000));
        let b = to_nanos(UNIX_EPOCH + std::time::Duration::from_secs(5000));
        let want = vec![
            // d1->src/f.rs: 45 duplicate live edges collapsed to ONE (source=45, valid_from=earliest).
            ("d1".to_string(), "src/f.rs".to_string(), 45, f),
            // a different FILE is a distinct relationship, its own single live edge.
            ("d1".to_string(), "src/g.rs".to_string(), 47, g),
            // a different DECISION is a distinct relationship, its own single live edge.
            ("d2".to_string(), "src/f.rs".to_string(), 46, b),
        ];
        assert_eq!(
            live_governs(&rebuilt),
            want,
            "a rebuild collapses the 45 duplicate live edges to exactly ONE per (from, rel, to, tier); distinct relationships each survive as their own single live edge"
        );

        // REBUILDABLE - a rebuild is a pure, reproducible function of the log: folding the SAME log
        // into ANOTHER fresh, empty projection re-derives the identical single-edge-per-key set.
        let rebuilt_again = Projector::open(":memory:", "test").unwrap();
        fold_log(&rebuilt_again);
        assert_eq!(
            live_governs(&rebuilt_again),
            want,
            "rebuilding the same log from scratch re-derives the identical deduped live edges"
        );
    }

    /// `nodes_of_kind` lists exactly the nodes of the kind it is asked for, ordered by id and
    /// scoped to the projector's own project: the decisions a fold projects, the artifacts they
    /// govern and none for a kind nothing folded.
    #[test]
    fn nodes_of_kind_lists_one_kind_ordered_by_id_within_the_project() {
        let backend = tempfile::tempdir().unwrap();
        let db = backend.path().join("graph.db");
        let p = Projector::open(db.to_str().unwrap(), "alpha").unwrap();
        let other = Projector::open(db.to_str().unwrap(), "beta").unwrap();
        apply_decision(&p, 1, "d2", "keep the reads narrow", &["b.rs"], "");
        apply_decision_by(&p, 2, "d1", "keep the walk scoped", &["a.rs"], "planner");
        apply_decision(&other, 1, "dz", "another project's", &["c.rs"], "");
        let cases: [(&str, &[&str]); 3] = [
            (KIND_ARTIFACT, &["a.rs", "b.rs"]),
            (KIND_DECISION, &["d1", "d2"]),
            (KIND_LESSON, &[]),
        ];
        for (kind, ids) in cases {
            let listed: Vec<String> = p
                .nodes_of_kind(kind)
                .unwrap()
                .into_iter()
                .map(|n| n.id)
                .collect();
            assert_eq!(listed, ids, "{kind}");
        }
    }

    #[test]
    fn whole_reads_the_full_projection_excludes_invalidated_edges_and_scopes_by_project() {
        // Spec 45, criterion 2: `whole()` is the DIRECT read the `/api/graph` provider consults -
        // the entire live projection, NO seed and NO reachability walk. It must (a) return every
        // node and every currently-valid edge, (b) EXCLUDE an edge invalidated by a supersede
        // (`valid_to` set), (c) be scoped to its own project on a shared backend, and (d) sort
        // deterministically.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();

        // Project p1: d1 governs a.rs, then d2 supersedes d1 and ALSO governs a.rs (so d1's GOVERNS
        // edge is invalidated while d2's stays live), plus a decision d3 governing b.rs.
        let p1 = Projector::open(path, "p1").unwrap();
        apply_decision(&p1, 1, "d1", "old", &["a.rs"], "");
        apply_decision(&p1, 2, "d2", "new", &["a.rs"], "d1");
        apply_decision(&p1, 3, "d3", "run", &["b.rs"], "");

        // Project p2 on the SAME backend file: its nodes must never leak into p1's whole read.
        let p2 = Projector::open(path, "p2").unwrap();
        apply_decision(&p2, 1, "pz", "other-project", &["z.rs"], "");

        let g = p1.whole().unwrap();

        // (a) every p1 node is present - the decisions and the files they govern - WITHOUT any
        // seed.
        for id in ["d1", "d2", "d3", "a.rs", "b.rs"] {
            assert!(
                g.nodes.iter().any(|n| n.id == id),
                "whole() returns node {id}, got {:?}",
                g.nodes
            );
        }
        // (c) p2's nodes are NOT visible through p1's scoped whole read.
        assert!(
            !g.nodes.iter().any(|n| n.id == "pz" || n.id == "z.rs"),
            "whole() is project-scoped: p2 nodes never leak into p1, got {:?}",
            g.nodes
        );

        // (a) the live GOVERNS edges (from d2 and d3) are returned.
        for (from, to) in [("d2", "a.rs"), ("d3", "b.rs")] {
            assert!(
                g.edges
                    .iter()
                    .any(|e| e.from == from && e.to == to && e.rel == REL_GOVERNS),
                "the live GOVERNS edge {from} -> {to} is returned, got {:?}",
                g.edges
            );
        }
        // (b) the supersede-invalidated GOVERNS edge (from d1) is EXCLUDED by the valid_to filter.
        assert!(
            !g.edges
                .iter()
                .any(|e| e.from == "d1" && e.rel == REL_GOVERNS),
            "the supersede-invalidated GOVERNS edge is excluded, got {:?}",
            g.edges
        );

        // (d) deterministic: nodes come back sorted by id.
        let ids: Vec<String> = g.nodes.iter().map(|n| n.id.clone()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(
            ids, sorted,
            "whole() returns nodes sorted by id, got {ids:?}"
        );
    }

    /// Every edge from `from`, as `(to, rel, valid_to)`, read STRAIGHT from the table - INCLUDING
    /// invalidated rows (a set `valid_to`) that the live `subgraph` filter hides. This is what lets
    /// a test prove supersede-not-delete: a superseded edge is RETAINED with `valid_to` stamped, so
    /// a historical / as-of reader still reaches it, not removed.
    fn edges_from(p: &Projector, from: &str) -> Vec<(String, String, Option<i64>)> {
        raw_rows(
            p,
            "SELECT to_id, rel, valid_to FROM edges
             WHERE from_id = ?1 ORDER BY rel, to_id, valid_from",
            [from],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get(2)?)),
        )
    }

    /// Every row `sql` selects straight off the projector's own connection, mapped by `row`.
    fn raw_rows<T, C: FromIterator<T>>(
        p: &Projector,
        sql: &str,
        params: impl rusqlite::Params,
        row: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    ) -> C {
        let conn = p.conn.lock().unwrap();
        let mut stmt = conn.prepare(sql).unwrap();
        stmt.query_map(params, row)
            .unwrap()
            .collect::<Result<C, _>>()
            .unwrap()
    }

    /// Every row of every table but the ledger's - the `applied` positions and the `caught_up`
    /// cursor over them - straight off the projector's own connection, as `<table>: <columns>`,
    /// sorted, so two dumps compare whatever order a table hands its rows in.
    fn projection_but_ledger(p: &Projector) -> Vec<String> {
        let tables: Vec<String> = raw_rows(
            p,
            "SELECT name FROM sqlite_master
              WHERE type = 'table' AND name NOT IN ('applied', 'caught_up')
                AND name NOT LIKE 'sqlite_%'",
            [],
            |r| r.get(0),
        );
        let mut rows: Vec<String> = Vec::new();
        for table in tables {
            let found: Vec<String> = raw_rows(p, &format!("SELECT * FROM {table}"), [], |r| {
                let width = r.as_ref().column_count();
                Ok((0..width)
                    .map(|i| format!("{:?}", r.get_ref(i).unwrap()))
                    .collect::<Vec<_>>()
                    .join(" | "))
            });
            rows.extend(found.into_iter().map(|row| format!("{table}: {row}")));
        }
        rows.sort();
        rows
    }

    #[test]
    fn folding_each_episodic_type_writes_its_applied_row_and_leaves_the_projection_unchanged() {
        // Spec 107, criterion 1 (THE CLASSES ARE ONE TABLE): an episodic type is one whose fold
        // changes neither the live projection nor the fold state, `FileTouched` and `GateVerdict`
        // through their no-op arms and the rest through none. Over a seeded graph, folding one
        // event of each type of `retention::EPISODIC_TYPES`, its payload naming a seeded file,
        // leaves every table but `applied` row for row as it stood and adds exactly its own
        // position to `applied`.
        let p = Projector::open(":memory:", "test").unwrap();
        let mut pos = crate::test_support::seed_two_subsystems(&p);
        apply_decision(&p, pos, "d1", "keep", &["src/combat/hit.rs"], "");
        let seeded = projection_but_ledger(&p);
        for table in ["nodes: ", "edges: ", "node_assertions: "] {
            assert!(
                seeded.iter().any(|row| row.starts_with(table)),
                "the seeded projection holds rows of {table}so an unchanged dump proves something"
            );
        }
        for type_ in rigger_domain::retention::EPISODIC_TYPES {
            pos += 1;
            let mut applied: Vec<i64> = raw_rows(
                &p,
                "SELECT position FROM applied ORDER BY position",
                [],
                |r| r.get(0),
            );
            apply_json(
                &p,
                pos,
                type_,
                serde_json::json!({
                    "id": "e1", "unit": "u1", "by": "rust-engineer", "path": "src/combat/hit.rs",
                    "gate": "cargo test", "pass": true, "artifact": "src/combat/hit.rs",
                }),
            );
            assert_eq!(
                projection_but_ledger(&p),
                seeded,
                "folding {type_} changes no table but applied"
            );
            applied.push(i64::try_from(pos).unwrap());
            let after: Vec<i64> = raw_rows(
                &p,
                "SELECT position FROM applied ORDER BY position",
                [],
                |r| r.get(0),
            );
            assert_eq!(
                after, applied,
                "folding {type_} adds exactly its own applied row"
            );
        }
    }

    #[test]
    fn a_blast_radius_computed_event_folds_to_nothing_idempotently() {
        // spec 16 unit 3: BlastRadiusComputed is PURE AUDIT - the projector matches no fold arm
        // for it (it falls to the `_ => {}` sink), so it adds NO node and NO edge, and re-applying
        // the SAME position (a replay) stays a no-op. This is what lets the audit ride the shared
        // stream without perturbing the context graph the reviewers read.
        let p = Projector::open(":memory:", "test").unwrap();
        let payload = serde_json::json!({
            "id": "u1",
            "unit": "u1",
            "precise": ["a.rs"],
            "safe": ["a.rs", "b.rs"],
            "serialize": false,
            "index_stamp": "h/v",
        });
        let mut e = Event::new(
            crate::conductor::TYPE_BLAST_RADIUS_COMPUTED,
            serde_json::to_vec(&payload).unwrap(),
        );
        e.position = 1;
        crate::test_support::folds(&p, std::slice::from_ref(&e));
        crate::test_support::folds(&p, std::slice::from_ref(&e)); // same position, replayed: still a no-op
        for seed in [["u1"], ["a.rs"], ["b.rs"]] {
            let g = p
                .subgraph(&seed.iter().map(|s| s.to_string()).collect::<Vec<_>>(), 2)
                .unwrap();
            assert!(
                g.nodes.is_empty() && g.edges.is_empty(),
                "a BlastRadiusComputed event folds to no node/edge; got {g:?}"
            );
        }
    }

    #[test]
    fn decision_fold_projects_no_agent_node_or_decided_edge() {
        // De-noise (spec 43): the acting persona is run machinery, not the target project, so a
        // DecisionMade - even carrying an event actor - projects NO KIND_AGENT node and NO REL_DECIDED
        // attribution edge. Its CONTENT survives: the decision node and its GOVERNS edge to the code.
        let p = Projector::open(":memory:", "test").unwrap();
        let payload = serde_json::json!({"id": "d1", "summary": "x", "governs": ["mod.rs"]});
        let mut e = Event::new(TYPE_DECISION_MADE, serde_json::to_vec(&payload).unwrap());
        e.position = 1;
        e.meta.insert(META_ACTOR.to_string(), "agent-7".to_string());
        crate::test_support::folds(&p, std::slice::from_ref(&e));
        let g = p.subgraph(&["d1".to_string()], 2).unwrap();
        // Content survives.
        assert!(
            g.nodes
                .iter()
                .any(|n| n.id == "d1" && n.kind == KIND_DECISION),
            "the decision content node survives the de-noise"
        );
        assert!(
            g.edges
                .iter()
                .any(|x| x.rel == REL_GOVERNS && x.from == "d1" && x.to == "mod.rs"),
            "the decision's GOVERNS edge to the code it concerns survives"
        );
        // Machinery is gone.
        assert!(
            !g.nodes.iter().any(|n| n.kind == KIND_AGENT),
            "no KIND_AGENT node is projected for the acting persona"
        );
        assert!(
            !g.edges.iter().any(|x| x.rel == REL_DECIDED),
            "no REL_DECIDED agent-attribution edge is projected"
        );
    }

    #[test]
    fn review_finding_creates_a_finding_node_about_each_file() {
        // A ReviewFinding folds into a KIND_FINDING node carrying its summary (and the reviewer
        // as a node ATTRIBUTE), and an ABOUT edge to each file it concerns. The finding is
        // reachable from the file it is ABOUT - the same traversal that returns the decisions
        // GOVERNING the file - so a later reviewer grounded on that file retrieves it through the
        // graph, not via hand-threaded prompts. De-noise (spec 43): the reviewer's provenance is
        // NOT projected as a KIND_AGENT node or a REL_RAISED edge (that agent attribution is run
        // machinery); the `by` reviewer remains only as the finding node's `by` attribute.
        let p = Projector::open(":memory:", "test").unwrap();
        let payload = serde_json::json!({
            "id": "f1",
            "by": "tech-lens",
            "unit": "u1",
            "summary": "the new path skips the buffer authority",
            "about": ["combat.rs"],
        });
        let mut e = Event::new(TYPE_REVIEW_FINDING, serde_json::to_vec(&payload).unwrap());
        e.position = 1;
        crate::test_support::folds(&p, std::slice::from_ref(&e));

        // Reachable from the file it is ABOUT.
        let g = p.subgraph(&["combat.rs".to_string()], 2).unwrap();
        let n = g
            .nodes
            .iter()
            .find(|n| n.id == "f1")
            .expect("the finding node is reachable from the file it is ABOUT");
        assert_eq!(n.kind, KIND_FINDING);
        assert_eq!(
            n.attrs.get("summary").map(String::as_str),
            Some("the new path skips the buffer authority")
        );
        assert_eq!(n.attrs.get("by").map(String::as_str), Some("tech-lens"));
        assert!(
            g.edges
                .iter()
                .any(|x| x.rel == REL_ABOUT && x.from == "f1" && x.to == "combat.rs"),
            "ABOUT(f1 -> combat.rs)"
        );
        // Machinery is gone: no reviewer agent node, no RAISED attribution edge.
        assert!(
            !g.nodes.iter().any(|n| n.kind == KIND_AGENT),
            "no KIND_AGENT node is projected for the reviewer"
        );
        assert!(
            !g.edges.iter().any(|x| x.rel == REL_RAISED),
            "no REL_RAISED agent-attribution edge is projected"
        );
    }

    /// A finding or lesson naming its location by `file`, with a line or without, is reached from
    /// that file's node as one naming it in `about` is (op-105-every-finding-reaches-the-graph):
    /// the fold reads the file as one more `about` reference, a cited line folding onto the file.
    #[test]
    fn a_record_naming_its_file_is_reached_from_that_files_node() {
        let p = Projector::open(":memory:", "test").unwrap();
        let events = [
            event_at(
                TYPE_REVIEW_FINDING,
                br#"{"id":"f-line","summary":"s","file":"src/combat.rs","line":12}"#,
                1,
            ),
            event_at(
                TYPE_REVIEW_FINDING,
                br#"{"id":"f-cited","summary":"s","file":"src/combat.rs:40-44"}"#,
                2,
            ),
            event_at(
                TYPE_LESSON_LEARNED,
                br#"{"id":"l-file","summary":"s","file":"src/combat.rs"}"#,
                3,
            ),
        ];
        crate::test_support::folds(&p, &events);

        let g = p.subgraph(&["src/combat.rs".to_string()], 2).unwrap();
        let mut about: Vec<&str> = g
            .edges
            .iter()
            .filter(|x| x.rel == REL_ABOUT && x.to == "src/combat.rs")
            .map(|x| x.from.as_str())
            .collect();
        about.sort_unstable();
        assert_eq!(
            about,
            ["f-cited", "f-line", "l-file"],
            "each record is ABOUT the file it names, reached from that file's node"
        );
    }

    #[test]
    fn review_finding_projects_no_raised_edge_even_with_an_event_actor() {
        // De-noise (spec 43): a ReviewFinding carrying an event actor still projects NO KIND_AGENT
        // node and NO REL_RAISED edge - the agent attribution is run machinery, dropped for both
        // the `by` reviewer and the actor override. Only the finding's CONTENT node survives.
        let p = Projector::open(":memory:", "test").unwrap();
        let payload = serde_json::json!({
            "id": "f1", "summary": "x", "about": ["a.rs"],
        });
        let mut e = Event::new(TYPE_REVIEW_FINDING, serde_json::to_vec(&payload).unwrap());
        e.position = 1;
        e.meta
            .insert(META_ACTOR.to_string(), "adversary".to_string());
        crate::test_support::folds(&p, std::slice::from_ref(&e));
        let g = p.subgraph(&["f1".to_string()], 2).unwrap();
        assert!(
            g.nodes
                .iter()
                .any(|n| n.id == "f1" && n.kind == KIND_FINDING),
            "the finding content node survives the de-noise"
        );
        assert!(
            !g.nodes.iter().any(|n| n.kind == KIND_AGENT),
            "no KIND_AGENT node is projected for the actor"
        );
        assert!(
            !g.edges.iter().any(|x| x.rel == REL_RAISED),
            "no REL_RAISED edge is projected even when an event actor is present"
        );
    }

    #[test]
    fn review_finding_fold_is_idempotent_per_position() {
        // A replayed ReviewFinding (same position) must not double the ABOUT edge.
        let p = Projector::open(":memory:", "test").unwrap();
        let payload = serde_json::json!({
            "id": "f1", "by": "lens", "summary": "x", "about": ["a.rs"],
        });
        for _ in 0..2 {
            let mut e = Event::new(TYPE_REVIEW_FINDING, serde_json::to_vec(&payload).unwrap());
            e.position = 1; // same position, replayed
            crate::test_support::folds(&p, std::slice::from_ref(&e));
        }
        let g = p.subgraph(&["a.rs".to_string()], 2).unwrap();
        let about = g
            .edges
            .iter()
            .filter(|x| x.rel == REL_ABOUT && x.from == "f1")
            .count();
        assert_eq!(
            about, 1,
            "a replayed finding must not double the ABOUT edge"
        );
    }

    /// Emit a ReviewFinding in the EXACT shape production records (the spec 11 review
    /// protocol): `{id, by, summary, about}` on the data, plus the emitting spawn's id on
    /// `meta.spawn` (the key `rigger emit` and the MCP server stamp on every real emit) -
    /// and NOTHING on `data.unit`, because a real finding never carries one. So a discard
    /// fold that keyed on a `$.unit` attr would match nothing here, exactly as it matches
    /// nothing in production; the fold must key on a field production actually sets.
    fn apply_review_finding(
        p: &Projector,
        pos: u64,
        id: &str,
        by: &str,
        spawn: &str,
        about: &[&str],
    ) {
        let payload = serde_json::json!({
            "id": id, "by": by, "summary": "x", "about": about,
        });
        let mut e = Event::new(TYPE_REVIEW_FINDING, serde_json::to_vec(&payload).unwrap())
            .with_meta(crate::spawn::META_SPAWN, spawn);
        e.position = pos;
        crate::test_support::folds(p, std::slice::from_ref(&e));
    }

    #[test]
    fn an_explicitly_discarded_finding_is_invalidated_upheld_and_undisposed_stay_live() {
        // Spec 25, criterion 1 (disposition-expiry, the DISCARD trigger): a finding the
        // adjudicator NAMES in its verdict line's `discarded` array is DISCARDED - folding
        // that adjudicator SpawnResult sets valid_to on the finding's RAISED/ABOUT edges
        // (invalidate, never delete - mirroring the decision-supersession arm), so the live
        // subgraph filter (valid_to IS NULL) stops returning it. The findings are emitted in
        // the EXACT production shape (id/by/summary/about + meta.spawn, NO data.unit), so this
        // proves the fold fires against what production actually records, not a hand-injected
        // unit attr. A finding the verdict UPHELD stays live, and so does a finding named in
        // NEITHER upheld NOR discarded: the discard keys on the explicit `discarded` array,
        // never the complement of `upheld`, so a reject's own still-open motivating findings
        // survive for the remediation to see.
        let p = Projector::open(":memory:", "test").unwrap();
        apply_review_finding(&p, 1, "f-discard", "lens:tech", "u1/lens:tech#0", &["a.rs"]);
        apply_review_finding(&p, 2, "f-upheld", "lens:sdet", "u1/lens:sdet#0", &["a.rs"]);
        apply_review_finding(&p, 3, "f-open", "lens:tech", "u1/lens:tech#0", &["a.rs"]);

        // Before the verdict, every finding is reachable from the file it is ABOUT.
        let before = p.subgraph(&["a.rs".to_string()], 2).unwrap();
        for id in ["f-discard", "f-upheld", "f-open"] {
            assert!(
                before.nodes.iter().any(|n| n.id == id),
                "{id} present before the verdict"
            );
        }

        // The adjudicator UPHOLDS f-upheld and explicitly DISCARDS f-discard; f-open is named
        // in neither - a reject's still-open motivating finding that must survive.
        let verdict = r#"{"verdict":"reject","upheld":["f-upheld"],"discarded":["f-discard"],"cause":"genuine-defect"}"#;
        let mut e = crate::spawn::SpawnResult::ok("u1/adjudicator#0", verdict)
            .to_event()
            .unwrap();
        e.position = 4;
        crate::test_support::folds(&p, std::slice::from_ref(&e));

        let after = p.subgraph(&["a.rs".to_string()], 2).unwrap();
        assert!(
            !after.nodes.iter().any(|n| n.id == "f-discard"),
            "the explicitly discarded finding is pruned from the live subgraph (its edges were invalidated)"
        );
        assert!(
            !after
                .edges
                .iter()
                .any(|x| x.from == "f-discard" || x.to == "f-discard"),
            "no live edge touches the discarded finding"
        );
        assert!(
            after.nodes.iter().any(|n| n.id == "f-upheld"),
            "the UPHELD finding stays live"
        );
        assert!(
            after.nodes.iter().any(|n| n.id == "f-open"),
            "a finding named in NEITHER upheld nor discarded stays live - the discard keys on \
             the explicit `discarded` array, never the complement of upheld"
        );
    }

    #[test]
    fn a_verdict_that_names_no_discarded_array_expires_nothing() {
        // The over-invalidation the discard MUST NOT do (spec 25 c1): the discard set is the
        // EXPLICIT `discarded` array, never the complement of `upheld`. An approve that
        // upholds one finding and names no `discarded` array expires NOTHING - every finding
        // the review raised, upheld or not, stays live. Real verdicts routinely omit `upheld`
        // entirely (56/234 adjudications approve with none), so a complement-of-upheld fold
        // would sweep a whole review here; keying on `discarded` cannot.
        let p = Projector::open(":memory:", "test").unwrap();
        apply_review_finding(&p, 1, "f-kept", "lens:tech", "u1/lens:tech#0", &["a.rs"]);
        apply_review_finding(
            &p,
            2,
            "f-also-kept",
            "lens:sdet",
            "u1/lens:sdet#0",
            &["a.rs"],
        );

        let verdict = r#"{"verdict":"approve","upheld":["f-kept"]}"#;
        let mut e = crate::spawn::SpawnResult::ok("u1/adjudicator#0", verdict)
            .to_event()
            .unwrap();
        e.position = 3;
        crate::test_support::folds(&p, std::slice::from_ref(&e));

        let after = p.subgraph(&["a.rs".to_string()], 2).unwrap();
        for id in ["f-kept", "f-also-kept"] {
            assert!(
                after.nodes.iter().any(|n| n.id == id),
                "{id} stays live - a verdict that names no `discarded` array expires nothing \
                 (the discard is never the complement of upheld)"
            );
        }
    }

    fn apply_adjudication(p: &Projector, pos: u64, spawn: &str, verdict: &str) {
        let mut e = crate::spawn::SpawnResult::ok(spawn, verdict)
            .to_event()
            .unwrap();
        e.position = pos;
        crate::test_support::folds(p, std::slice::from_ref(&e));
    }

    fn apply_unit_integrated(p: &Projector, pos: u64, unit: &str, commit: &str) {
        // Production shape: the conductor emits `{"id": <unit>, "commit": ...}` at every
        // UNIT_INTEGRATED site (the `id` key, NOT `unit`). Building it this way proves the
        // fold parses what production actually records, not a hand-tuned `unit` payload.
        apply_json(
            p,
            pos,
            TYPE_UNIT_INTEGRATED,
            serde_json::json!({"id": unit, "commit": commit}),
        );
    }

    #[test]
    fn an_upheld_finding_expires_when_its_unit_integrates_but_stays_live_until_then() {
        // Spec 25, criterion 2 (disposition-expiry, the UPHELD-AND-ADDRESSED trigger): a
        // finding the adjudicator UPHELD is RESOLVED once the unit that owns it INTEGRATES
        // (addresses it). The adjudicator's SpawnResult MARKS each upheld finding-of-unit
        // (disposition=upheld + the unit it belongs to) without invalidating; folding that
        // unit's TYPE_UNIT_INTEGRATED then sets valid_to on the marked finding's RAISED/ABOUT
        // edges, so the live subgraph filter (valid_to IS NULL) stops returning it. Two guards
        // this proves: an upheld finding whose unit has NOT integrated stays live (marking
        // alone expires nothing), and the trigger fires against the EXACT production payload -
        // the conductor emits UNIT_INTEGRATED with an `id` key, and the findings carry NO
        // `data.unit` (only the adjudicator spawn id names the unit), so a fold keyed on a
        // hand-injected `unit` attr would be vacuous here.
        let p = Projector::open(":memory:", "test").unwrap();
        // f-a is upheld for unit u1 (which will integrate); f-b is upheld for unit u2 (which
        // will NOT integrate). Emitted in production shape: id/by/summary/about + meta.spawn,
        // no data.unit. (After the spec 43 de-noise a finding carries only its ABOUT edge - the
        // RAISED agent-attribution edge is no longer projected - so integration invalidates ABOUT.)
        apply_review_finding(&p, 1, "f-a", "lens:sdet", "u1/lens:sdet#0", &["a.rs"]);
        apply_review_finding(&p, 2, "f-b", "lens:arch", "u2/lens:arch#0", &["b.rs"]);

        // Each unit's adjudicator UPHOLDS its finding. This MARKS the finding (records the
        // disposition and its owning unit, taken from the adjudicator spawn id) but must NOT
        // invalidate anything yet - the finding is not addressed until its unit integrates.
        apply_adjudication(
            &p,
            3,
            "u1/adjudicator#0",
            r#"{"verdict":"approve","upheld":["f-a"]}"#,
        );
        apply_adjudication(
            &p,
            4,
            "u2/adjudicator#0",
            r#"{"verdict":"approve","upheld":["f-b"]}"#,
        );

        // Marking alone expires nothing: both upheld findings stay live until their unit lands.
        let marked = p
            .subgraph(&["a.rs".to_string(), "b.rs".to_string()], 2)
            .unwrap();
        for id in ["f-a", "f-b"] {
            assert!(
                marked.nodes.iter().any(|n| n.id == id),
                "{id} is upheld but stays live until its unit integrates"
            );
        }

        // u1 integrates: its upheld finding f-a is now ADDRESSED and expires.
        apply_unit_integrated(&p, 5, "u1", "commitsha");

        let after = p
            .subgraph(&["a.rs".to_string(), "b.rs".to_string()], 2)
            .unwrap();
        assert!(
            !after.nodes.iter().any(|n| n.id == "f-a"),
            "the upheld finding of the INTEGRATED unit is pruned (its edges were invalidated)"
        );
        assert!(
            !after.edges.iter().any(|x| x.from == "f-a" || x.to == "f-a"),
            "no live edge touches the addressed finding"
        );
        assert!(
            after.nodes.iter().any(|n| n.id == "f-b"),
            "an upheld finding whose unit has NOT integrated stays live"
        );
        // Spec 43 criterion 3 (LIFECYCLE survives the de-noise): disposition-expiry fired above
        // even though NO KIND_UNIT node was ever created for u1 - the invalidation reads the
        // finding's `$.unit` string attribute (a token), not a unit node.
        let (nodes, _) = all_nodes_edges(&p);
        assert!(
            !nodes.iter().any(|(_, k)| k == KIND_UNIT),
            "no KIND_UNIT node is projected, yet the integrate still drove disposition-expiry"
        );
    }

    #[test]
    fn a_discard_under_run_a_never_suppresses_the_same_finding_re_raised_under_a_later_run_b() {
        // Spec 25, criterion 3 (disposition-expiry, RUN-SCOPING - the DISCARD trigger): expiry
        // is by DISPOSITION, not by run age. A finding DISCARDED under run A has its RAISED /
        // ABOUT edges invalidated (valid_to set), so it drops from the live subgraph - but
        // re-raising the SAME finding id under a LATER run B must return it LIVE again. This
        // holds by fold-position semantics, not a stored run label: invalidate_finding_edges
        // only touches edges that currently hold (valid_to IS NULL) AT FOLD TIME, and a re-raise
        // add_edges a FRESH row (valid_to NULL) that run A's already-folded discard never saw.
        // So run A's invalidation is scoped to run A's own edges and can never suppress a run B
        // re-raise. This criterion OWNS that run-scoping guarantee; it does NOT own the discard
        // trigger (criterion 1 does) - the discard here is only the disposition-under-A
        // precondition.
        let p = Projector::open(":memory:", "test").unwrap();

        // Run A raises f-x about a.rs; it is live.
        apply_review_finding(&p, 1, "f-x", "lens:tech", "u1/lens:tech#0", &["a.rs"]);
        assert!(
            p.subgraph(&["a.rs".to_string()], 2)
                .unwrap()
                .nodes
                .iter()
                .any(|n| n.id == "f-x"),
            "f-x is live after run A raises it"
        );

        // Run A's adjudicator DISCARDS f-x: its edges are invalidated, so it drops from the live
        // subgraph. (Criterion 1's trigger, used here only to set the run-A disposition.)
        apply_adjudication(
            &p,
            2,
            "u1/adjudicator#0",
            r#"{"verdict":"reject","discarded":["f-x"]}"#,
        );
        assert!(
            !p.subgraph(&["a.rs".to_string()], 2)
                .unwrap()
                .nodes
                .iter()
                .any(|n| n.id == "f-x"),
            "run A's discard invalidates f-x's edges, so it drops from the live subgraph"
        );

        // A LATER run B re-raises the SAME finding id. The re-raise appends FRESH valid_to-NULL
        // RAISED / ABOUT edges that run A's earlier discard never touched, so f-x is LIVE again -
        // run A's disposition never suppresses a run B re-raise (expiry by disposition, not run
        // age).
        apply_review_finding(&p, 3, "f-x", "lens:tech", "u1-run-b/lens:tech#0", &["a.rs"]);
        let after_b = p.subgraph(&["a.rs".to_string()], 2).unwrap();
        assert!(
            after_b.nodes.iter().any(|n| n.id == "f-x"),
            "the SAME finding re-raised under a later run B is returned LIVE by subgraph - run A's \
             discard never suppresses a B re-raise"
        );
        assert!(
            after_b
                .edges
                .iter()
                .any(|x| x.from == "f-x" || x.to == "f-x"),
            "run B's re-raise created a fresh live edge for f-x (run A's invalidation stayed \
             scoped to run A's edges)"
        );
    }

    #[test]
    fn an_upheld_mark_never_expires_the_same_finding_re_raised_before_its_unit_integrates() {
        // Spec 25, criterion 3 (disposition-expiry, RUN-SCOPING - the UPHELD-AND-ADDRESSED
        // trigger): a finding UPHELD for unit u1 under run A is MARKED (disposition=upheld,
        // unit=u1) and expires only when u1 INTEGRATES. If a LATER run B re-raises the SAME
        // finding between the mark and the integrate, that re-raise re-runs ensure_node (the
        // TYPE_REVIEW_FINDING arm's own call, which unconditionally re-supplies `unit` -
        // production's `ReviewFinding.unit` defaults to an empty string). Corrected (spec 86
        // criterion 2 round 3, arch-u86c2-r2-ensure-node-merge-silently-narrows-spec25-
        // disposition-mark-erasure): `ensure_node`'s `ON CONFLICT` no longer whole-blob-replaces
        // via COALESCE - it `json_patch`-MERGES (spec 86 criterion 2 round 2) - so this re-raise
        // does NOT clear the mark wholesale; it overwrites only the ONE key it re-supplies
        // (`unit`, back to empty), which is enough on its own to knock the marked-for-u1 SELECT's
        // `unit = ?2` out of matching (`disposition` survives untouched, unread anywhere else).
        // Either way the finding no longer matches when u1 integrates and stays LIVE, while a
        // sibling still-marked finding (never re-raised) is correctly expired. This proves run
        // A's upheld disposition never over-invalidates a run B re-raise (the cross-run
        // over-invalidation guard) - true under BOTH the old and the new `ensure_node` mechanism,
        // for two different reasons; see the `TYPE_UNIT_INTEGRATED` fold arm's own updated doc
        // for the fragility the NEW reason carries. This criterion OWNS that run-scoping
        // guarantee; it does NOT own the upheld-and-addressed trigger (criterion 2 does).
        let p = Projector::open(":memory:", "test").unwrap();

        // Run A raises two findings about a.rs, both upheld for u1: f-reraised (which run B will
        // re-raise) and f-control (which nothing re-raises), emitted in production shape.
        apply_review_finding(
            &p,
            1,
            "f-reraised",
            "lens:sdet",
            "u1/lens:sdet#0",
            &["a.rs"],
        );
        apply_review_finding(&p, 2, "f-control", "lens:arch", "u1/lens:arch#0", &["a.rs"]);
        apply_adjudication(
            &p,
            3,
            "u1/adjudicator#0",
            r#"{"verdict":"approve","upheld":["f-control","f-reraised"]}"#,
        );

        // A LATER run B re-raises ONLY f-reraised. The re-raise's ensure_node call json_patch-
        // merges its attrs, overwriting `unit` (back to empty) while leaving `disposition`
        // untouched, and appends fresh live edges.
        apply_review_finding(
            &p,
            4,
            "f-reraised",
            "lens:sdet",
            "u1-run-b/lens:sdet#0",
            &["a.rs"],
        );

        // u1 integrates: it expires only the findings STILL marked upheld-for-u1. f-control is
        // still marked and expires; f-reraised's mark was cleared by run B's re-raise, so it is
        // untouched and stays LIVE.
        apply_unit_integrated(&p, 5, "u1", "commitsha");

        let after = p.subgraph(&["a.rs".to_string()], 2).unwrap();
        assert!(
            after.nodes.iter().any(|n| n.id == "f-reraised"),
            "the finding re-raised under a later run B stays LIVE when u1 integrates - the \
             re-raise's own ensure_node call overwrote the mark's unit token, so run A's \
             disposition never over-invalidates a B re-raise"
        );
        assert!(
            after
                .edges
                .iter()
                .any(|x| x.from == "f-reraised" || x.to == "f-reraised"),
            "run B's re-raise left f-reraised with a fresh live edge"
        );
        assert!(
            !after.nodes.iter().any(|n| n.id == "f-control"),
            "the sibling upheld finding that was NEVER re-raised is correctly expired on integrate \
             - proving the integrate genuinely fires and f-reraised's survival is the run-scoping \
             effect, not a vacuous no-op"
        );
    }

    #[test]
    fn unit_started_folds_to_no_node_or_edge() {
        // De-noise (spec 43): a unit, its assigned agent, and its dependency edges are all run
        // machinery, not the target project, so a UnitStarted folds to NOTHING in the graph - no
        // KIND_UNIT node, no KIND_AGENT node, no REL_ASSIGNED_TO / REL_BLOCKS edge. The event stays
        // in the log, where the run-tree (units/stages) and metrics read it. Seeding the graph on
        // any of the ids the event named returns an empty neighborhood.
        let p = Projector::open(":memory:", "test").unwrap();
        let payload =
            serde_json::json!({"unit": "u2", "criterion": "c", "agent": "impl", "needs": ["u1"]});
        let mut e = Event::new(TYPE_UNIT_STARTED, serde_json::to_vec(&payload).unwrap());
        e.position = 1;
        crate::test_support::folds(&p, std::slice::from_ref(&e));
        for seed in [["u2"], ["u1"], ["impl"]] {
            let g = p
                .subgraph(&seed.iter().map(|s| s.to_string()).collect::<Vec<_>>(), 2)
                .unwrap();
            assert!(
                g.nodes.is_empty() && g.edges.is_empty(),
                "a UnitStarted event folds to no node/edge; seeding {seed:?} gave {g:?}"
            );
        }
    }

    #[test]
    fn machinery_is_gone_after_folding_a_full_run() {
        // Spec 43 criterion 1 (OWNS the machinery drop): after folding a run's FileTouched,
        // UnitStarted, UnitIntegrated, GateVerdict, DecisionMade, and ReviewFinding events, the
        // projection contains NO harness machinery anywhere - no KIND_AGENT / KIND_UNIT / KIND_GATE
        // node, no REL_TOUCHES edge, and no agent-attribution edge (RAISED or DECIDED, nor the
        // ASSIGNED_TO / BLOCKS / GATED_BY machinery edges). The graph models the TARGET PROJECT
        // (its code and the design memory about it), not the loop's own bookkeeping.
        let p = Projector::open(":memory:", "test").unwrap();

        // DecisionMade with an acting agent (event actor), governing a file.
        let mut d = Event::new(
            TYPE_DECISION_MADE,
            serde_json::to_vec(&serde_json::json!({
                "id": "d1", "summary": "x", "governs": ["combat.rs"], "supersedes": ""
            }))
            .unwrap(),
        );
        d.position = 1;
        d.meta
            .insert(META_ACTOR.to_string(), "rust-engineer".to_string());
        crate::test_support::folds(&p, std::slice::from_ref(&d));

        // FileTouched (agent touches file).
        let mut ft = Event::new(
            TYPE_FILE_TOUCHED,
            serde_json::to_vec(&serde_json::json!({ "path": "combat.rs", "by": "rust-engineer" }))
                .unwrap(),
        );
        ft.position = 2;
        crate::test_support::folds(&p, std::slice::from_ref(&ft));

        // GateVerdict on the file.
        let mut gv = Event::new(
            TYPE_GATE_VERDICT,
            serde_json::to_vec(
                &serde_json::json!({ "gate": "cargo test", "pass": true, "artifact": "combat.rs" }),
            )
            .unwrap(),
        );
        gv.position = 3;
        crate::test_support::folds(&p, std::slice::from_ref(&gv));

        // UnitStarted (unit assigned to an agent, blocked by another unit).
        let mut us = Event::new(
            TYPE_UNIT_STARTED,
            serde_json::to_vec(&serde_json::json!({
                "unit": "u2", "criterion": "c", "agent": "impl", "needs": ["u1"]
            }))
            .unwrap(),
        );
        us.position = 4;
        crate::test_support::folds(&p, std::slice::from_ref(&us));

        // ReviewFinding raised by a reviewer about the file.
        let mut rf = Event::new(
            TYPE_REVIEW_FINDING,
            serde_json::to_vec(&serde_json::json!({
                "id": "f1", "by": "tech-lens", "unit": "u2", "summary": "y", "about": ["combat.rs"]
            }))
            .unwrap(),
        );
        rf.position = 5;
        crate::test_support::folds(&p, std::slice::from_ref(&rf));

        // UnitIntegrated.
        let mut ui = Event::new(
            TYPE_UNIT_INTEGRATED,
            serde_json::to_vec(&serde_json::json!({ "id": "u2", "commit": "abc" })).unwrap(),
        );
        ui.position = 6;
        crate::test_support::folds(&p, std::slice::from_ref(&ui));

        let (nodes, edges) = all_nodes_edges(&p);
        for kind in [KIND_AGENT, KIND_UNIT, KIND_GATE] {
            assert!(
                !nodes.iter().any(|(_, k)| k == kind),
                "no {kind} machinery node is projected; got nodes {nodes:?}"
            );
        }
        for rel in [
            REL_TOUCHES,
            REL_RAISED,
            REL_DECIDED,
            REL_ASSIGNED_TO,
            REL_BLOCKS,
            REL_GATED_BY,
        ] {
            assert!(
                !edges.iter().any(|(_, r, _)| r == rel),
                "no {rel} machinery edge is projected; got edges {edges:?}"
            );
        }
    }

    #[test]
    fn content_survives_the_de_noise() {
        // Spec 43 criterion 2 (OWNS content preservation): the same fold that drops machinery STILL
        // produces the KIND_DECISION / KIND_FINDING / KIND_LESSON content nodes and their edges to
        // the code they concern (GOVERNS for a decision, ABOUT for a finding and a lesson). Only the
        // agent attribution is absent. It does NOT own the machinery drop (criterion 1).
        let p = Projector::open(":memory:", "test").unwrap();
        apply_decision(&p, 1, "d1", "use the shared authority", &["combat.rs"], "");
        apply_review_finding(&p, 2, "f1", "tech-lens", "u1/tech-lens#0", &["combat.rs"]);
        let mut le = Event::new(
            TYPE_LESSON_LEARNED,
            serde_json::to_vec(
                &serde_json::json!({ "id": "l1", "summary": "z", "about": ["combat.rs"] }),
            )
            .unwrap(),
        );
        le.position = 3;
        crate::test_support::folds(&p, std::slice::from_ref(&le));

        let g = p.subgraph(&["combat.rs".to_string()], 2).unwrap();
        // Content nodes survive, reachable from the code they concern.
        assert!(
            g.nodes
                .iter()
                .any(|n| n.id == "d1" && n.kind == KIND_DECISION),
            "the decision content node survives"
        );
        assert!(
            g.nodes
                .iter()
                .any(|n| n.id == "f1" && n.kind == KIND_FINDING),
            "the finding content node survives"
        );
        assert!(
            g.nodes
                .iter()
                .any(|n| n.id == "l1" && n.kind == KIND_LESSON),
            "the lesson content node survives"
        );
        // Content edges to the code survive.
        assert!(
            g.edges
                .iter()
                .any(|e| e.rel == REL_GOVERNS && e.from == "d1" && e.to == "combat.rs"),
            "the decision's GOVERNS edge survives"
        );
        assert!(
            g.edges
                .iter()
                .any(|e| e.rel == REL_ABOUT && e.from == "f1" && e.to == "combat.rs"),
            "the finding's ABOUT edge survives"
        );
        assert!(
            g.edges
                .iter()
                .any(|e| e.rel == REL_ABOUT && e.from == "l1" && e.to == "combat.rs"),
            "the lesson's ABOUT edge survives"
        );
        // Only the agent attribution is absent.
        assert!(
            !g.nodes.iter().any(|n| n.kind == KIND_AGENT),
            "no agent attribution node survives"
        );
        assert!(
            !g.edges
                .iter()
                .any(|e| e.rel == REL_RAISED || e.rel == REL_DECIDED),
            "no agent attribution edge survives"
        );
    }

    #[test]
    fn consumers_read_the_log_not_the_dropped_machinery_nodes() {
        // Spec 43 criterion 4 (OWNS the safe-consumer guarantee): metrics folds the EVENT LOG, never
        // the graph, so dropping the unit/gate nodes cannot change what it reports. Fold a run's
        // UnitStarted + GateVerdicts into the de-noised projection AND compute metrics from the same
        // events: the graph carries NO KIND_UNIT / KIND_GATE node, yet metrics still counts the unit
        // from the log - proving the consumer never depended on the dropped nodes. It does NOT own
        // content preservation (criterion 2).
        let unit_started = {
            // Production shape: the conductor emits UnitStarted with an `id` and `agent` (metrics
            // keys on `id`), plus the `unit` the fold once read.
            let mut e = Event::new(
                TYPE_UNIT_STARTED,
                serde_json::to_vec(&serde_json::json!({
                    "id": "u1", "unit": "u1", "criterion": "c", "agent": "impl", "needs": []
                }))
                .unwrap(),
            );
            e.position = 1;
            e
        };
        let gate_pass = {
            let mut e = Event::new(
                TYPE_GATE_VERDICT,
                serde_json::to_vec(&serde_json::json!({ "gate": "cargo test", "pass": true }))
                    .unwrap(),
            );
            e.position = 2;
            e
        };
        let events = vec![unit_started, gate_pass];

        // Fold into the (de-noised) graph.
        let p = Projector::open(":memory:", "test").unwrap();
        crate::test_support::folds_each(&p, &events);
        let (nodes, _) = all_nodes_edges(&p);
        assert!(
            !nodes.iter().any(|(_, k)| k == KIND_UNIT || k == KIND_GATE),
            "the de-noised graph carries no unit/gate machinery node; got {nodes:?}"
        );

        // Metrics reads the LOG and is unaffected by the absent nodes.
        let m = crate::metrics::project(&events);
        assert_eq!(
            m.units_started, 1,
            "metrics counts the unit from the event log, not the (absent) graph node"
        );
    }

    #[test]
    fn aliases_collapse_synonyms_onto_one_node() {
        let p = Projector::open(":memory:", "test").unwrap();
        let alias = crate::test_support::alias_json("the editor", "content-editor");
        let mut ae = Event::new(TYPE_ALIAS_DEFINED, serde_json::to_vec(&alias).unwrap());
        ae.position = 1;
        crate::test_support::folds(&p, std::slice::from_ref(&ae));
        apply_decision(&p, 2, "d1", "x", &["the editor"], "");
        let g = p.subgraph(&["content-editor".to_string()], 2).unwrap();
        assert!(
            g.edges
                .iter()
                .any(|x| x.rel == REL_GOVERNS && x.from == "d1" && x.to == "content-editor"),
            "the alias must collapse 'the editor' onto 'content-editor'"
        );
        assert_eq!(
            p.resolve("the editor").unwrap().as_deref(),
            Some("content-editor")
        );
    }

    #[test]
    fn alias_unresolved_creates_a_node_marked_for_merge() {
        let p = Projector::open(":memory:", "test").unwrap();
        let payload = serde_json::json!({"mention": "some thing"});
        let mut e = Event::new(TYPE_ALIAS_UNRESOLVED, serde_json::to_vec(&payload).unwrap());
        e.position = 1;
        crate::test_support::folds(&p, std::slice::from_ref(&e));
        let g = p.subgraph(&["some thing".to_string()], 1).unwrap();
        let n = g
            .nodes
            .iter()
            .find(|n| n.id == "some thing")
            .expect("a node is created for an unresolved mention");
        assert_eq!(n.attrs.get("unresolved").map(String::as_str), Some("true"));
    }

    #[test]
    fn edge_valid_from_is_the_event_valid_time() {
        let p = Projector::open(":memory:", "test").unwrap();
        let vf = std::time::UNIX_EPOCH + std::time::Duration::from_secs(500);
        let payload = serde_json::json!({"id": "d1", "summary": "x", "governs": ["mod.rs"]});
        let mut e = Event::new(TYPE_DECISION_MADE, serde_json::to_vec(&payload).unwrap());
        e.position = 1;
        e.valid_from = vf;
        crate::test_support::folds(&p, std::slice::from_ref(&e));
        let g = p.subgraph(&["mod.rs".to_string()], 2).unwrap();
        let edge = g.edges.iter().find(|x| x.rel == REL_GOVERNS).unwrap();
        assert_eq!(
            edge.valid_from, 500_000_000_000,
            "the edge valid_from is the event's valid_from in nanos"
        );
    }

    /// The project stamped on every node row, as `(id, project)` pairs in `(id, project)`
    /// order. Spec 28 criterion 1's write tag lives on the raw `project` column; `subgraph`
    /// does not yet filter by it (that read filter is criterion 2), so this reads the column
    /// directly to prove the fold stamps it. Reads through the same connection, so on a shared
    /// backend it observes every project's committed rows.
    fn node_projects(p: &Projector) -> Vec<(String, String)> {
        raw_rows(
            p,
            "SELECT id, project FROM nodes ORDER BY id, project",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
    }

    /// The project stamped on every edge row, in insertion order. Same rationale as
    /// [`node_projects`]: the write tag is on the raw column, read directly.
    fn edge_projects(p: &Projector) -> Vec<String> {
        raw_rows(p, "SELECT project FROM edges ORDER BY id", [], |r| r.get(0))
    }

    #[test]
    fn every_node_and_edge_carries_the_projects_scope_on_fold() {
        // Spec 28, criterion 1 (the WRITE tag): a fold of the same events under project P tags
        // EVERY resulting node and edge with P, derived from the SAME plain project string
        // `Namespaced::new` uses for the `proj-<id>-` stream prefix - threaded in through the
        // scoped `Projector::open(path, project)` constructor. This is net-new node/edge state:
        // there was no project field before, so the whole value is the tag appearing on every
        // row of a fold.
        //
        // Proven three ways: (1) under project "alpha" every node/edge row reads project=alpha;
        // (2) the SAME events folded under "beta" tag "beta", so the scope is derived from the
        // constructor, not hard-coded; (3) on ONE SHARED backend the SAME seed id d1 coexists as
        // two distinct rows (one per project) - the composite (id, project) key that makes the
        // tag genuinely isolating state, which read-isolation (c2) and rebuild-under-scope (c4)
        // rely on. On a shared backend the two projects' events occupy DISTINCT global positions
        // (the `Namespaced` decorator scopes streams over one global log), so beta folds the
        // same-shaped event at a later position - the `applied` ledger never mistakes it for an
        // already-folded event.
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("graph.db");
        let shared = shared.to_str().unwrap();

        // A decision governing shared.rs, decided by agent-7: folds a decision node, an artifact
        // node (shared.rs), an agent node (agent-7), a DECIDED edge and a GOVERNS edge - every
        // node kind and both edge directions from one event.

        // (1) project "alpha": every row tagged "alpha".
        let alpha = Projector::open(shared, "alpha").unwrap();
        apply_decision_by(&alpha, 1, "d1", "x", &["shared.rs"], "agent-7");
        let a_nodes = node_projects(&alpha);
        assert!(
            a_nodes.iter().any(|(id, _)| id == "d1"),
            "the decision node was folded"
        );
        assert!(
            a_nodes.iter().all(|(_, proj)| proj == "alpha"),
            "every node carries project=alpha on fold, got {a_nodes:?}"
        );
        let a_edges = edge_projects(&alpha);
        assert!(!a_edges.is_empty(), "the fold produced edges");
        assert!(
            a_edges.iter().all(|proj| proj == "alpha"),
            "every edge carries project=alpha on fold, got {a_edges:?}"
        );

        // (2) same backend, project "beta": the SAME event (a later global position) tags "beta".
        let beta = Projector::open(shared, "beta").unwrap();
        apply_decision_by(&beta, 2, "d1", "x", &["shared.rs"], "agent-7");
        let all_nodes = node_projects(&beta);
        // (3) the SAME seed id d1 now exists under BOTH projects - two distinct rows on ONE
        // shared backend (the composite (id, project) key), never one overwriting the other.
        let d1_projects: Vec<&str> = all_nodes
            .iter()
            .filter(|(id, _)| id == "d1")
            .map(|(_, proj)| proj.as_str())
            .collect();
        assert_eq!(
            d1_projects,
            vec!["alpha", "beta"],
            "the same seed id d1 coexists as one row per project on a shared backend"
        );
        // beta genuinely tagged beta, and alpha's row is untouched by beta's fold.
        assert!(
            all_nodes
                .iter()
                .any(|(id, proj)| id == "shared.rs" && proj == "beta"),
            "beta's fold of shared.rs is tagged beta, got {all_nodes:?}"
        );
        assert!(
            all_nodes
                .iter()
                .any(|(id, proj)| id == "shared.rs" && proj == "alpha"),
            "alpha's shared.rs row is untouched by beta's fold, got {all_nodes:?}"
        );
    }

    /// The OLD schema a graph.db had before spec 28, with no `project` column and no recorded
    /// fold rule, seeded with a decision, the file it governs and the edge between them, the way
    /// pre-spec-28 code folded them.
    const PRE_PROJECT_GRAPH: &str = "
        CREATE TABLE nodes (id TEXT PRIMARY KEY, kind TEXT NOT NULL, attrs TEXT);
        CREATE TABLE edges (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          from_id TEXT NOT NULL, to_id TEXT NOT NULL, rel TEXT NOT NULL,
          valid_from INTEGER NOT NULL, valid_to INTEGER, source INTEGER NOT NULL
        );
        CREATE TABLE aliases (alias TEXT PRIMARY KEY, canonical_id TEXT NOT NULL);
        CREATE TABLE applied (position INTEGER PRIMARY KEY);
        INSERT INTO nodes (id, kind, attrs)
          VALUES ('old-d', 'decision', '{\"summary\":\"legacy\"}');
        INSERT INTO nodes (id, kind, attrs) VALUES ('old.rs', 'artifact', NULL);
        INSERT INTO edges (from_id, to_id, rel, valid_from, valid_to, source)
          VALUES ('old-d', 'old.rs', 'GOVERNS', 100, NULL, 1);";

    #[test]
    fn a_pre_project_graph_db_migrates_additively_backfilling_the_openers_identity() {
        // Spec 28 backward-compat (a GLOBAL CONSTRAINT criterion 1 owns as an ADDITIVE
        // migration): a graph.db written before the project scope existed has the OLD shape -
        // nodes(id PRIMARY KEY, kind, attrs) and edges(... source) with no `project` column.
        // Opening it with the scoped constructor migrates it in place WITHOUT wiping it:
        // existing rows survive, backfilled with the OPENER's identity (so a single-project
        // deployment reads identically once the criterion-2 filter lands), and the recreated
        // nodes table now carries the composite (id, project) key so a second project's
        // same-id fold coexists rather than overwriting.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();

        Connection::open(path)
            .unwrap()
            .execute_batch(PRE_PROJECT_GRAPH)
            .unwrap();

        // Open through the scoped constructor: the migration runs, backfilling the opener's id.
        let p = Projector::open(path, "legacy-proj").unwrap();

        // Existing rows survived AND are now tagged with the opener's identity.
        let nodes = node_projects(&p);
        assert!(
            nodes.contains(&("old-d".to_string(), "legacy-proj".to_string())),
            "the legacy decision node survived and is backfilled with the opener's identity, \
             got {nodes:?}"
        );
        assert!(
            nodes.contains(&("old.rs".to_string(), "legacy-proj".to_string())),
            "the legacy artifact node survived and is backfilled, got {nodes:?}"
        );
        assert_eq!(
            edge_projects(&p),
            vec!["legacy-proj".to_string()],
            "the legacy edge survived and is backfilled with the opener's identity"
        );
        // The migrated node's attrs are intact (the data was copied, not just the id).
        let g = p.subgraph(&["old.rs".to_string()], 2).unwrap();
        let d = g
            .nodes
            .iter()
            .find(|n| n.id == "old-d")
            .expect("the migrated decision is still reachable from the file it governs");
        assert_eq!(d.attrs.get("summary").map(String::as_str), Some("legacy"));

        // The recreated composite (id, project) key lets a DIFFERENT project fold the same id as
        // a distinct row - proving the migration produced the isolating schema, not just a
        // column. Reopening the same file re-runs the migration as a no-op (the column exists).
        let other = Projector::open(path, "other-proj").unwrap();
        let payload = serde_json::json!({"id": "old-d", "summary": "fresh", "governs": ["old.rs"]});
        let mut e = Event::new(TYPE_DECISION_MADE, serde_json::to_vec(&payload).unwrap());
        e.position = 2;
        crate::test_support::folds(&other, std::slice::from_ref(&e));
        let after = node_projects(&other);
        let old_d: Vec<&str> = after
            .iter()
            .filter(|(id, _)| id == "old-d")
            .map(|(_, proj)| proj.as_str())
            .collect();
        assert_eq!(
            old_d,
            vec!["legacy-proj", "other-proj"],
            "the same id old-d coexists across the migrated project and a new one (composite PK)"
        );
    }

    /// The `(from_id, to_id, project)` of every edge touching node `id`, in `id` (insertion)
    /// order. Spec 28 criterion 3 reads the raw `edges` table directly - `subgraph`'s read
    /// filter is criterion 2, not yet relied on here - so on a shared backend it observes
    /// EVERY project's edges that reference `id`, exactly what a cross-project prune leak
    /// would show.
    fn edges_touching(p: &Projector, id: &str) -> Vec<(String, String, String)> {
        raw_rows(
            p,
            "SELECT from_id, to_id, project FROM edges
             WHERE from_id = ?1 OR to_id = ?1 ORDER BY id",
            [id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            },
        )
    }

    #[test]
    fn prune_is_project_scoped_leaving_another_projects_same_id_node_intact() {
        // Spec 28, criterion 3 (the PRUNE scope): `Projector::prune` is the single graph-mutation
        // authority `rigger reset --runs` uses to shed a dead run's nodes. On a SHARED backend it
        // must delete ONLY the pruning project's nodes and edges - pruning project P's dead-run
        // node must leave project Q's node with the SAME seed id fully intact. Without the scope
        // the id-keyed DELETE reaches across projects and wipes Q's row (and Q's edges) too, since
        // the composite (id, project) key (criterion 1) lets both projects hold that id at once.
        //
        // Fixture: ONE shared graph.db file, two projects (alpha, beta), each folding the SAME
        // decision "drop-d" (governing shared.rs, decided by agent-7) so the SAME seed id exists
        // as one row per project, WITH edges in both directions touching it (agent-7 -DECIDED->
        // drop-d and drop-d -GOVERNS-> shared.rs). Prune "drop-d" through ALPHA only.
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("graph.db");
        let shared = shared.to_str().unwrap();

        let alpha = Projector::open(shared, "alpha").unwrap();
        apply_decision_by(&alpha, 1, "drop-d", "x", &["shared.rs"], "agent-7");
        // beta folds the SAME event at a later global position (the Namespaced decorator scopes
        // streams over one global log), so beta's drop-d is a distinct row on the one backend.
        let beta = Projector::open(shared, "beta").unwrap();
        apply_decision_by(&beta, 2, "drop-d", "x", &["shared.rs"], "agent-7");

        // Before: the SAME seed id drop-d exists as one row per project, and BOTH projects have
        // edges touching it - so the survival assertions below are non-vacuous.
        let before = node_projects(&alpha);
        let drop_before: Vec<&str> = before
            .iter()
            .filter(|(id, _)| id == "drop-d")
            .map(|(_, proj)| proj.as_str())
            .collect();
        assert_eq!(
            drop_before,
            vec!["alpha", "beta"],
            "drop-d coexists under both projects on the shared backend before prune, got {before:?}"
        );
        let edges_before = edges_touching(&alpha, "drop-d");
        assert!(
            edges_before.iter().any(|(_, _, proj)| proj == "alpha"),
            "alpha has edges touching drop-d before prune, got {edges_before:?}"
        );
        assert!(
            edges_before.iter().any(|(_, _, proj)| proj == "beta"),
            "beta has edges touching drop-d before prune, got {edges_before:?}"
        );

        // Prune drop-d through ALPHA's projector.
        let removed = alpha.prune(&["drop-d".to_string()], None).unwrap();
        assert_eq!(
            removed.nodes, 1,
            "prune removes EXACTLY alpha's one drop-d node, never reaching beta's same-id row"
        );

        // After: alpha's drop-d node is gone; beta's drop-d node (same id) is left fully intact.
        let after = node_projects(&alpha);
        assert!(
            !after
                .iter()
                .any(|(id, proj)| id == "drop-d" && proj == "alpha"),
            "alpha's drop-d node is pruned, got {after:?}"
        );
        assert!(
            after
                .iter()
                .any(|(id, proj)| id == "drop-d" && proj == "beta"),
            "beta's drop-d node with the SAME seed id is left fully intact, got {after:?}"
        );

        // Edges are project-scoped too: every alpha edge touching drop-d is swept, and every
        // beta edge touching drop-d survives - the prune never dangles or over-reaches across
        // projects.
        let edges_after = edges_touching(&alpha, "drop-d");
        assert!(
            !edges_after.iter().any(|(_, _, proj)| proj == "alpha"),
            "alpha's edges touching drop-d are all pruned, got {edges_after:?}"
        );
        assert!(
            !edges_after.is_empty() && edges_after.iter().all(|(_, _, proj)| proj == "beta"),
            "every surviving edge touching drop-d is beta's - beta's edges are untouched, \
             got {edges_after:?}"
        );
    }

    /// Fold a decision `id` (summary `summary`) governing `governs`, DECIDED by `actor`, at
    /// global `pos`. Used by the read-isolation test to seed two projects on ONE shared backend
    /// with the SAME seed ids but distinct project-scoped neighborhoods.
    fn apply_decision_by(
        p: &Projector,
        pos: u64,
        id: &str,
        summary: &str,
        governs: &[&str],
        actor: &str,
    ) {
        let decision = decision_json(id, summary, governs, "");
        apply_json_as(p, pos, TYPE_DECISION_MADE, decision, Some(actor));
    }

    #[test]
    fn subgraph_isolates_reads_to_the_current_project_on_a_shared_backend() {
        // Spec 28, criterion 2 (the READ filter): one graph store holding TWO projects' folds
        // returns, via subgraph, ONLY the current project's nodes - even when both projects
        // contain a node with the SAME seed id. This mirrors, for the graph, what
        // `Namespaced::scope_filter` does for streams. The write tag (criterion 1) already puts
        // every row on a `(id, project)` key; this proves the read side never crosses the scope.
        //
        // One shared graph.db, two Projectors ("alpha", "beta"). BOTH fold a decision "d1" (and
        // an artifact "shared.rs") - the SAME seed ids under both projects - plus a
        // project-UNIQUE neighbor decision ("only-alpha" / "only-beta") governing the same file.
        // Each decision carries an actor, but the de-noise (spec 43) drops it: no agent node is
        // projected, so scoping is proven over the surviving decision/file nodes. On a shared
        // backend the two projects occupy
        // DISTINCT global positions (the `Namespaced` decorator scopes streams over one global
        // log), so distinct positions keep the shared `applied` ledger from mistaking beta's
        // fold for an already-applied one.
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("graph.db");
        let shared = shared.to_str().unwrap();

        let alpha = Projector::open(shared, "alpha").unwrap();
        apply_decision_by(&alpha, 1, "d1", "alpha-d1", &["shared.rs"], "agent-alpha");
        apply_decision_by(&alpha, 2, "only-alpha", "x", &["shared.rs"], "agent-alpha");

        let beta = Projector::open(shared, "beta").unwrap();
        apply_decision_by(&beta, 3, "d1", "beta-d1", &["shared.rs"], "agent-beta");
        apply_decision_by(&beta, 4, "only-beta", "y", &["shared.rs"], "agent-beta");

        // alpha's read: seeded on the file BOTH projects share.
        let ag = alpha.subgraph(&["shared.rs".to_string()], 2).unwrap();
        // (1) same seed id d1 in two projects -> ONLY alpha's d1, exactly once.
        let d1s: Vec<&str> = ag
            .nodes
            .iter()
            .filter(|n| n.id == "d1")
            .map(|n| n.attrs.get("summary").map(String::as_str).unwrap_or(""))
            .collect();
        assert_eq!(
            d1s,
            vec!["alpha-d1"],
            "alpha's read returns ONLY alpha's d1 (never beta's), exactly one row, got {ag:?}"
        );
        // (2) exactly one shared.rs node (not beta's duplicate).
        assert_eq!(
            ag.nodes.iter().filter(|n| n.id == "shared.rs").count(),
            1,
            "the shared seed node appears once, scoped to alpha, got {ag:?}"
        );
        // (3) the traversal never crosses into beta's neighborhood.
        for leaked in ["only-beta", "agent-beta", "beta-d1"] {
            assert!(
                !ag.nodes.iter().any(|n| n.id == leaked),
                "alpha's read must not surface beta-only node {leaked}, got {ag:?}"
            );
            assert!(
                !ag.edges.iter().any(|e| e.from == leaked || e.to == leaked),
                "alpha's read must not surface any edge touching beta-only {leaked}, got {ag:?}"
            );
        }
        // alpha's own neighborhood is intact (the filter isolates, it does not empty the graph).
        assert!(
            ag.nodes.iter().any(|n| n.id == "only-alpha"),
            "alpha's own node only-alpha stays reachable under scope, got {ag:?}"
        );
        // De-noise (spec 43): the DECIDING agent is never a node, in either project's read.
        assert!(
            !ag.nodes.iter().any(|n| n.kind == KIND_AGENT),
            "no KIND_AGENT node is projected (the actor is dropped), got {ag:?}"
        );

        // beta's read is the mirror image over the SAME shared backend.
        let bg = beta.subgraph(&["shared.rs".to_string()], 2).unwrap();
        let bd1s: Vec<&str> = bg
            .nodes
            .iter()
            .filter(|n| n.id == "d1")
            .map(|n| n.attrs.get("summary").map(String::as_str).unwrap_or(""))
            .collect();
        assert_eq!(
            bd1s,
            vec!["beta-d1"],
            "beta's read returns ONLY beta's d1, exactly one row, got {bg:?}"
        );
        for leaked in ["only-alpha", "agent-alpha", "alpha-d1"] {
            assert!(
                !bg.nodes.iter().any(|n| n.id == leaked),
                "beta's read must not surface alpha-only node {leaked}, got {bg:?}"
            );
        }
        assert!(
            bg.nodes.iter().any(|n| n.id == "only-beta"),
            "beta's own node only-beta stays reachable under scope, got {bg:?}"
        );
        assert!(
            !bg.nodes.iter().any(|n| n.kind == KIND_AGENT),
            "no KIND_AGENT node is projected in beta's read either, got {bg:?}"
        );
    }

    #[test]
    fn resolve_is_project_scoped_on_a_shared_backend() {
        // Spec 28, criterion 2 (read isolation, the resolve read): `ContextGraph::resolve`'s
        // node-existence fallback is a read of the nodes table, so on a shared backend it must
        // answer for the CURRENT project only - a node id that exists solely under project beta
        // must resolve to None for project alpha, never a cross-project false-positive. The
        // alias path is unaffected (the `aliases` table carries no project column and stays
        // shared); only the node-existence fallback is scoped.
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("graph.db");
        let shared = shared.to_str().unwrap();

        let alpha = Projector::open(shared, "alpha").unwrap();
        apply_decision(&alpha, 1, "a-only", "x", &["shared.rs"], "");
        let beta = Projector::open(shared, "beta").unwrap();
        apply_decision(&beta, 2, "b-only", "y", &["shared.rs"], "");

        // Each project resolves its OWN node id...
        assert_eq!(
            alpha.resolve("a-only").unwrap().as_deref(),
            Some("a-only"),
            "alpha resolves its own node"
        );
        assert_eq!(
            beta.resolve("b-only").unwrap().as_deref(),
            Some("b-only"),
            "beta resolves its own node"
        );
        // ...but NOT the other project's node, even though it exists on the shared backend.
        assert_eq!(
            alpha.resolve("b-only").unwrap(),
            None,
            "alpha must not resolve beta-only node (cross-project existence leak)"
        );
        assert_eq!(
            beta.resolve("a-only").unwrap(),
            None,
            "beta must not resolve alpha-only node (cross-project existence leak)"
        );
    }

    #[test]
    fn migrate_project_rekeys_rows_so_read_isolation_survives_identity_mint() {
        // Spec 28 GC5 (backward-compat): `Projector::migrate_project` is the graph analog of
        // `rename_stream_prefix` for the spec-09 identity mint. A single-project deployment folds
        // rows under its basename identity "oldname"; the mint renames its streams to a durable
        // identity, but the graph folds incrementally so the renamed streams are never re-folded
        // and the pre-mint rows keep the legacy scope. Once the read filter (criterion 2) scopes
        // reads to the minted identity, those rows orphan unless re-keyed. Re-keying moves ONLY
        // the named scope's rows, leaving another project's SAME-id rows on the shared backend
        // fully intact.
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("graph.db");
        let shared = shared.to_str().unwrap();

        // Pre-mint history under the legacy basename "oldname" (decision + its governed file).
        let legacy = Projector::open(shared, "oldname").unwrap();
        apply_decision(&legacy, 1, "pre-d", "s", &["pre.rs"], "");
        // An unrelated project "sibling" holds the SAME node id "pre-d" on the one shared backend.
        let sibling = Projector::open(shared, "sibling").unwrap();
        apply_decision(&sibling, 2, "pre-d", "sib", &["pre.rs"], "");

        // The minted projector re-keys ONLY the legacy scope's rows to the minted identity.
        let minted = Projector::open(shared, "mint123").unwrap();
        let moved = minted.migrate_project("oldname", "mint123").unwrap();
        assert_eq!(
            moved, 2,
            "both the pre-mint decision and its governed file re-key (2 nodes moved)"
        );

        // The minted read now returns the pre-mint history - it did NOT orphan.
        let g = minted.subgraph(&["pre.rs".to_string()], 2).unwrap();
        assert!(
            g.nodes.iter().any(|n| n.id == "pre-d"),
            "the re-keyed pre-mint decision is reachable under the minted identity, got {g:?}"
        );
        assert_eq!(
            minted.resolve("pre-d").unwrap().as_deref(),
            Some("pre-d"),
            "the pre-mint node resolves under the minted identity after migration"
        );

        // The legacy scope is now empty, and the sibling project's SAME-id rows are untouched.
        let projs = node_projects(&minted);
        assert!(
            !projs.iter().any(|(_, p)| p == "oldname"),
            "no row keeps the legacy scope after migration, got {projs:?}"
        );
        assert!(
            projs.iter().any(|(id, p)| id == "pre-d" && p == "sibling"),
            "another project's same-id row is left fully intact, got {projs:?}"
        );
        assert_eq!(
            projs
                .iter()
                .filter(|(id, p)| id == "pre-d" && p == "mint123")
                .count(),
            1,
            "exactly one minted pre-d row exists (the re-key never duplicated), got {projs:?}"
        );

        // Idempotent: a re-open after the migration re-keys nothing (the legacy scope is empty).
        assert_eq!(
            minted.migrate_project("oldname", "mint123").unwrap(),
            0,
            "re-keying again moves nothing"
        );
    }

    /// A node's kind settles on its first assertion's, promoted from a bare artifact only, by
    /// the first promoting kind after it ([`settled_kind`]).
    #[test]
    fn a_nodes_kind_settles_on_its_first_assertion_promoted_only_from_a_bare_artifact() {
        let cases: [(&[&str], Option<&str>); 7] = [
            (&[], None),
            (&[KIND_ARTIFACT], Some(KIND_ARTIFACT)),
            (&[KIND_ARTIFACT, KIND_FILE], Some(KIND_FILE)),
            (
                &[KIND_ARTIFACT, KIND_ARTIFACT, KIND_CODE_ENTITY],
                Some(KIND_CODE_ENTITY),
            ),
            (&[KIND_ARTIFACT, KIND_DECISION], Some(KIND_ARTIFACT)),
            (&[KIND_FILE, KIND_ARTIFACT], Some(KIND_FILE)),
            (&[KIND_CODE_ENTITY, KIND_FILE], Some(KIND_CODE_ENTITY)),
        ];
        for (kinds, settled) in cases {
            assert_eq!(settled_kind(kinds.iter().copied()), settled, "{kinds:?}");
        }
    }

    /// Spec 117, A MIGRATED PROJECT STAYS LIVE: [`Projector::migrate_project`] re-keys every
    /// project-keyed table the file holds, in one transaction, and skips each it does not hold.
    mod migrate_project {
        use super::leaf_keys_resolution::write_directly;
        use super::*;

        /// Every table of the projection that carries a `project` column.
        const PROJECT_KEYED: [&str; 3] = ["nodes", "edges", "node_assertions"];

        /// How many rows of each of `tables` in `p`'s file carry the scope `project`.
        fn rows_under(p: &Projector, tables: &[&str], project: &str) -> Vec<(String, i64)> {
            tables
                .iter()
                .map(|table| {
                    let sql = format!("SELECT count(*) FROM {table} WHERE project = ?1");
                    let n: Vec<i64> = raw_rows(p, &sql, [project], |r| r.get(0));
                    (table.to_string(), n[0])
                })
                .collect()
        }

        /// `tables` each paired with its row count from `counts`, in order.
        fn counted(tables: &[&str], counts: &[i64]) -> Vec<(String, i64)> {
            tables
                .iter()
                .zip(counts)
                .map(|(table, n)| (table.to_string(), *n))
                .collect()
        }

        /// `p`'s migration of its own project to `minted`: the nodes it moved, or its error's
        /// text.
        fn migrate_to_minted(p: &Projector) -> Result<usize, String> {
            p.migrate_project(&p.project, "minted").map_err(|e| e.0)
        }

        /// A fresh projector holding the decision `d1` governing `src/f.rs`: two nodes, one edge
        /// and the two node assertions behind them ([`HELD`]).
        fn holding_a_decision() -> Projector {
            let p = crate::test_support::fold_events([]);
            crate::test_support::apply_decision(&p, 1, "d1", "governs f", &["src/f.rs"], "");
            p
        }

        /// The rows of each [`PROJECT_KEYED`] table [`holding_a_decision`] leaves.
        const HELD: [i64; 3] = [2, 1, 2];

        /// A file that owes its rebuild is opened as it stands, so it may hold `nodes` and `edges`
        /// and none of the ledgers; the migration moves both and skips every table the file does
        /// not hold.
        #[test]
        fn an_owed_store_holding_only_nodes_and_edges_migrates_both() {
            let owed = holding_a_decision();
            let from = owed.project.clone();
            write_directly(&owed, |tx, _| {
                tx.execute_batch(&format!("{DROP_LEDGERS} PRAGMA user_version = 0;"))
                    .unwrap();
            });
            assert!(owed.rebuild_owed().unwrap());
            let tables = ["nodes", "edges"];
            assert_eq!(migrate_to_minted(&owed), Ok(2));
            assert_eq!(
                rows_under(&owed, &tables, "minted"),
                counted(&tables, &HELD[..2])
            );
            assert_eq!(rows_under(&owed, &tables, &from), counted(&tables, &[0, 0]));
        }

        /// Every row of `p`'s file but its `applied` ledger, its fold rule, and whether its
        /// `nodes` and `edges` carry a `project` column.
        fn as_it_stands(p: &Projector) -> (Vec<String>, i64, [bool; 2]) {
            let conn = p.conn.lock().unwrap();
            let keyed =
                ["nodes", "edges"].map(|table| column_exists(&conn, table, "project").unwrap());
            let rule = fold_rule(&conn).unwrap();
            drop(conn);
            (projection_but_ledger(p), rule, keyed)
        }

        /// A graph.db folded before spec 28 owes its rebuild and is opened as it stands, its
        /// `nodes` and `edges` with no `project` column; the migration moves nothing and leaves
        /// the file as planted.
        #[test]
        fn a_pre_project_owed_store_migrates_nothing_and_stands_as_planted() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("graph.db");
            let path = path.to_str().unwrap();
            Connection::open(path)
                .unwrap()
                .execute_batch(&format!(
                    "{PRE_PROJECT_GRAPH} INSERT INTO applied VALUES (1);"
                ))
                .unwrap();
            let owed = Projector::open(path, "legacy").unwrap();
            assert!(owed.rebuild_owed().unwrap());
            let planted = as_it_stands(&owed);
            assert_eq!(
                (planted.0.len(), planted.1, planted.2),
                (3, 0, [false, false]),
                "the open leaves the two nodes and the edge as planted, at no fold rule"
            );
            assert_eq!(
                owed.migrate_project("legacy", "minted").map_err(|e| e.0),
                Ok(0)
            );
            assert_eq!(as_it_stands(&owed), planted);
        }

        /// The error a trigger planted by [`refusing_the_rekey_of`] raises.
        const REFUSED: &str = "re-key refused";

        /// A re-key that fails in `table` fails the whole migration with its error and moves
        /// nothing, in that table or any other: a trigger refuses every update of `table`.
        fn refusing_the_rekey_of(table: &str) {
            let p = holding_a_decision();
            let from = p.project.clone();
            write_directly(&p, |tx, _| {
                tx.execute_batch(&format!(
                    "CREATE TRIGGER refuse BEFORE UPDATE ON {table}
                     BEGIN SELECT RAISE(ABORT, '{REFUSED}'); END;"
                ))
                .unwrap();
            });
            assert_eq!(
                migrate_to_minted(&p),
                Err(REFUSED.to_string()),
                "the refused re-key of {table} fails the migration"
            );
            assert_eq!(
                (
                    rows_under(&p, &PROJECT_KEYED, &from),
                    rows_under(&p, &PROJECT_KEYED, "minted")
                ),
                (
                    counted(&PROJECT_KEYED, &HELD),
                    counted(&PROJECT_KEYED, &[0; 3])
                ),
                "a refused re-key of {table} leaves every row under the old scope"
            );
        }

        /// The edges, the nodes and a ledger: each re-keyed by its own update.
        #[test]
        fn a_refused_rekey_of_the_edges_the_nodes_or_a_ledger_moves_nothing() {
            for table in ["edges", "nodes", "node_assertions"] {
                refusing_the_rekey_of(table);
            }
        }

        /// A column check that fails fails the whole migration with its error and moves nothing:
        /// `node_assertions` stands as a view over a table the file lacks, so reading its columns
        /// fails.
        #[test]
        fn a_failed_column_check_fails_the_migration_and_moves_nothing() {
            let p = holding_a_decision();
            let from = p.project.clone();
            write_directly(&p, |tx, _| {
                tx.execute_batch(
                    "DROP VIEW IF EXISTS live_node_assertions; DROP TABLE node_assertions;
                     CREATE VIEW node_assertions AS SELECT * FROM gone;",
                )
                .unwrap();
            });
            assert_eq!(
                migrate_to_minted(&p),
                Err("no such table: main.gone".to_string())
            );
            let tables = ["nodes", "edges"];
            assert_eq!(rows_under(&p, &tables, &from), counted(&tables, &HELD[..2]));
        }

        /// [`PROJECT_KEYED`] is every table of a fresh graph file that carries a `project` column,
        /// read from the file's schema, and the migration re-keys each: `nodes`, `edges` and
        /// every [`PROJECT_KEYED_LEDGERS`] table.
        #[test]
        fn the_project_keyed_tables_are_every_table_carrying_a_project_column() {
            let p = crate::test_support::fold_events([]);
            let keyed: Vec<String> = raw_rows(
                &p,
                "SELECT m.name FROM sqlite_master m JOIN pragma_table_info(m.name) c
                  WHERE m.type = 'table' AND c.name = 'project'
                  ORDER BY m.name",
                [],
                |r| r.get(0),
            );
            let mut oracle = PROJECT_KEYED.map(String::from).to_vec();
            oracle.sort();
            assert_eq!(keyed, oracle);
            assert_eq!(PROJECT_KEYED[2..], PROJECT_KEYED_LEDGERS);
        }

        /// A store holding every table migrates each, moving every row to the minted scope.
        #[test]
        fn a_store_holding_every_keyed_table_migrates_each() {
            let p = holding_a_decision();
            let from = p.project.clone();
            assert_eq!(migrate_to_minted(&p), Ok(2));
            assert_eq!(
                rows_under(&p, &PROJECT_KEYED, "minted"),
                counted(&PROJECT_KEYED, &HELD)
            );
            assert_eq!(
                rows_under(&p, &PROJECT_KEYED, &from),
                counted(&PROJECT_KEYED, &[0; 3])
            );
        }
    }

    #[test]
    fn subgraph_traversal_and_edge_scope_are_pinned_independently_of_the_node_fetch() {
        // Spec 28 criterion 2, hardening. The base isolation test distinguishes projects by node
        // id, so the node-fetch scope alone could mask a traversal that wrongly crossed into
        // another project OR an edge fetched from another project. This fixture makes the
        // TRAVERSAL scope (`e.project` in the recursive CTE) and the EDGE-FETCH scope (`project`
        // on the edge read) each INDEPENDENTLY necessary, keyed on rows whose ids ALSO exist
        // under the reading project so the node-fetch scope cannot hide either leak.
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("graph.db");
        let shared = shared.to_str().unwrap();

        // In alpha, shared.rs connects to a-seed, and `bridge` exists but connects ONLY to
        // alpha-side.rs - there is NO alpha edge between shared.rs and bridge.
        let alpha = Projector::open(shared, "alpha").unwrap();
        apply_decision(&alpha, 1, "a-seed", "x", &["shared.rs"], "");
        apply_decision(&alpha, 2, "bridge", "x", &["alpha-side.rs"], "");

        // In beta the SAME ids exist, PLUS a beta edge between shared.rs and bridge (bridge
        // governs shared.rs) and a second beta edge between a-seed and shared.rs. Every endpoint
        // id also exists under alpha, so either edge would leak into alpha's read if a scope
        // clause were dropped - the node-fetch scope alone cannot hide it.
        let beta = Projector::open(shared, "beta").unwrap();
        apply_decision(&beta, 3, "a-seed", "y", &["shared.rs"], "");
        apply_decision(&beta, 4, "bridge", "y", &["shared.rs"], "");

        // Non-vacuous fixture: bridge exists under BOTH projects; beta has a shared.rs<->bridge
        // edge and alpha does not.
        let projs = node_projects(&alpha);
        assert!(
            projs.iter().any(|(id, p)| id == "bridge" && p == "alpha"),
            "bridge exists under alpha (so a leaked traversal would surface it), got {projs:?}"
        );
        assert!(
            projs.iter().any(|(id, p)| id == "bridge" && p == "beta"),
            "bridge exists under beta, got {projs:?}"
        );
        let bridge_edges = edges_touching(&alpha, "bridge");
        assert!(
            bridge_edges
                .iter()
                .any(|(f, t, p)| p == "beta" && (f == "shared.rs" || t == "shared.rs")),
            "beta has an edge between shared.rs and bridge (the traversal bait), got {bridge_edges:?}"
        );
        assert!(
            !bridge_edges
                .iter()
                .any(|(f, t, p)| p == "alpha" && (f == "shared.rs" || t == "shared.rs")),
            "alpha has NO edge between shared.rs and bridge, got {bridge_edges:?}"
        );

        let ag = alpha.subgraph(&["shared.rs".to_string()], 2).unwrap();
        // (1) TRAVERSAL scope: bridge is reachable from shared.rs ONLY via beta's edge, and its
        // id exists under alpha, so a dropped CTE `e.project` clause would surface alpha's bridge.
        assert!(
            !ag.nodes.iter().any(|n| n.id == "bridge"),
            "alpha's traversal must not cross beta's edge into `bridge`, got {ag:?}"
        );
        // (2) EDGE-FETCH scope: only alpha's own edge among the reached ids is returned. a-seed
        // and shared.rs both exist under beta with a beta edge between them, so a dropped edge
        // `project` clause would also return that beta edge. Exactly one alpha edge spans the set.
        assert_eq!(
            ag.edges.len(),
            1,
            "exactly alpha's one edge among the reached nodes is returned (never beta's), got {ag:?}"
        );
        assert!(
            ag.edges
                .iter()
                .all(|e| (e.from == "a-seed" && e.to == "shared.rs")
                    || (e.from == "shared.rs" && e.to == "a-seed")),
            "the returned edge is alpha's a-seed<->shared.rs edge, got {ag:?}"
        );
    }

    #[test]
    fn rebuilding_from_a_two_project_log_re_derives_two_correctly_scoped_subgraphs() {
        // Spec 28, criterion 4 (rebuild-under-scope). The graph is a REBUILDABLE projection of
        // the event log (addendum section 2.1), never hand-maintained state. Rebuilding it FROM
        // SCRATCH out of a single log that carries TWO projects' events re-derives two correctly-
        // scoped subgraphs - each project sees ONLY its own nodes - with NO MANUAL BACKFILL. The
        // project tag is re-DERIVED on every fold from the SAME injected identity that scopes the
        // streams, never stored as a mutable side fact a rebuild would drop. This owns rebuild-
        // under-scope; it leans on (but does not own) the write tag (criterion 1) and the read
        // filter (criterion 2).
        //
        // The shared global log interleaves both projects at DISTINCT positions (the `Namespaced`
        // decorator scopes each project's streams over one global log). A from-scratch rebuild
        // replays that whole log, routing each event to a Projector scoped to its OWNING project
        // against the SAME graph.db - exactly what re-deriving a shared-backend graph from
        // position 0 does after the graph.db is discarded. No prune / migrate / UPDATE is ever
        // called: the scope is a pure product of folding, which is what "no manual backfill"
        // means for a rebuild.

        // One canonical two-project log. Each entry is (owning project, event): the shape a
        // shared-backend replay sees - one global stream, every event attributable to its project
        // by its `Namespaced` prefix, at a DISTINCT global position. The seed ids "d1" and
        // "shared.rs" live in BOTH projects, so a rebuild that lost scope would MERGE them; a
        // correct rebuild keeps them apart.
        let two_project_log = || -> Vec<(&'static str, Event)> {
            let ev = |pos: u64, id: &str, summary: &str, actor: &str| -> Event {
                let payload = serde_json::json!({
                    "id": id, "summary": summary, "governs": ["shared.rs"], "supersedes": "",
                });
                let mut e = Event::new(TYPE_DECISION_MADE, serde_json::to_vec(&payload).unwrap());
                e.position = pos;
                e.meta.insert(META_ACTOR.to_string(), actor.to_string());
                e
            };
            vec![
                ("alpha", ev(1, "d1", "alpha-d1", "agent-alpha")),
                ("beta", ev(2, "d1", "beta-d1", "agent-beta")),
                ("alpha", ev(3, "only-alpha", "x", "agent-alpha")),
                ("beta", ev(4, "only-beta", "y", "agent-beta")),
            ]
        };

        // Rebuild the WHOLE log into a fresh, EMPTY graph.db at `path`: replay every event,
        // folding each into a Projector scoped to its owning project (one scoped view per project
        // over the shared backend, kept in a `BTreeMap` so the rebuild is deterministic per the
        // Global constraint). A fresh db has no pre-existing rows, so nothing is ever backfilled -
        // the scope each row carries comes SOLELY from the fold. Returns, per project, the set of
        // node ids that project's scoped `subgraph` reaches from the file both share: the rebuilt,
        // scoped projection.
        let rebuild = |path: &str| -> BTreeMap<String, BTreeSet<String>> {
            let mut projectors: BTreeMap<String, Projector> = BTreeMap::new();
            for (proj, e) in two_project_log() {
                let p = projectors
                    .entry(proj.to_string())
                    .or_insert_with(|| Projector::open(path, proj).unwrap());
                crate::test_support::folds(&*p, std::slice::from_ref(&e));
            }
            projectors
                .iter()
                .map(|(proj, p)| {
                    let g = p.subgraph(&["shared.rs".to_string()], 2).unwrap();
                    let ids = g
                        .nodes
                        .iter()
                        .map(|n| n.id.clone())
                        .collect::<BTreeSet<_>>();
                    (proj.clone(), ids)
                })
                .collect()
        };

        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("graph.db");
        let scoped = rebuild(first.to_str().unwrap());

        // Each project's rebuilt subgraph reaches EXACTLY its own nodes - never the other's, even
        // though both share the seed ids d1 and shared.rs. Exact-set equality pins BOTH failure
        // directions at once: no beta node leaks into alpha's rebuild (over-reach), and none of
        // alpha's own nodes go missing (under-derivation). Each decision carries an actor, but the
        // de-noise (spec 43) drops it - no agent node is re-derived - so the scoped set is the
        // shared file, the shared decision id, and the project-unique decision, exercising node,
        // edge, and traversal re-derivation together.
        let want = |own_decision: &str| -> BTreeSet<String> {
            ["shared.rs", "d1", own_decision]
                .iter()
                .map(|s| s.to_string())
                .collect()
        };
        assert_eq!(
            scoped.get("alpha"),
            Some(&want("only-alpha")),
            "the rebuilt alpha subgraph reaches EXACTLY alpha's own nodes (no beta leak, none \
             missing), got {scoped:?}"
        );
        assert_eq!(
            scoped.get("beta"),
            Some(&want("only-beta")),
            "the rebuilt beta subgraph reaches EXACTLY beta's own nodes (no alpha leak, none \
             missing), got {scoped:?}"
        );
        // The shared seed d1 is re-derived under BOTH scopes as its OWN row (one per project),
        // never merged into one and never crossed over - the composite (id, project) key that
        // makes the projection isolating survives a from-scratch rebuild.
        assert!(
            scoped["alpha"].contains("d1") && scoped["beta"].contains("d1"),
            "the shared seed id d1 is re-derived under both project scopes, got {scoped:?}"
        );

        // REBUILDABLE: discard the graph.db and rebuild the SAME log from scratch into a DIFFERENT
        // fresh, empty db. The re-derived scoped projection is IDENTICAL - the scope is a pure,
        // reproducible function of the log, re-derived on every fold, not a mutable side fact a
        // rebuild would lose. (The first db is untouched by this second rebuild.)
        let dir2 = tempfile::tempdir().unwrap();
        let second = dir2.path().join("graph.db");
        let rebuilt_again = rebuild(second.to_str().unwrap());
        assert_eq!(
            rebuilt_again, scoped,
            "rebuilding the two-project log from scratch re-derives the identical scoped subgraphs"
        );
    }

    // ---- spec 29a criterion 2: the confidence tier on folded structural edges ----

    #[test]
    fn a_dev_loop_edge_folds_at_the_extracted_tier() {
        // Every non-code dev-loop edge (DECIDED / GOVERNS / ...) is an explicit fact on the log, so
        // it folds EXTRACTED (addendum 6.2) - the tier column is universal, not code-only.
        let p = Projector::open(":memory:", "test").unwrap();
        let mut e = Event::new(
            TYPE_DECISION_MADE,
            serde_json::to_vec(
                &serde_json::json!({"id": "d1", "summary": "x", "governs": ["combat.rs"]}),
            )
            .unwrap(),
        );
        e.position = 1;
        crate::test_support::folds(&p, std::slice::from_ref(&e));
        let g = p.subgraph(&["d1".to_string()], 2).unwrap();
        assert_eq!(
            edge_tier(&g, REL_GOVERNS, "combat.rs"),
            TIER_EXTRACTED,
            "a GOVERNS dev-loop edge folds EXTRACTED"
        );
    }

    #[test]
    fn a_pre_tier_graph_db_migrates_additively_backfilling_the_extracted_tier() {
        // Additive backward-compat (spec 29a, addendum 6.2, "migrate in place like
        // migrate_project_scope"): a graph.db written before the tier existed has an `edges` table
        // with no `tier` column. Opening it must migrate in place WITHOUT wiping it, backfilling
        // every existing edge to EXTRACTED - the correct tier for a dev-loop edge - so the edge
        // survives and reads back tiered.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();

        // Hand-build a project-scoped-but-tier-less edges table and seed a GOVERNS edge.
        {
            let conn = Connection::open(path).unwrap();
            conn.execute_batch(
                "CREATE TABLE nodes (
                   id TEXT NOT NULL, kind TEXT NOT NULL, attrs TEXT,
                   project TEXT NOT NULL DEFAULT '', PRIMARY KEY (id, project)
                 );
                 CREATE TABLE edges (
                   id INTEGER PRIMARY KEY AUTOINCREMENT,
                   from_id TEXT NOT NULL, to_id TEXT NOT NULL, rel TEXT NOT NULL,
                   valid_from INTEGER NOT NULL, valid_to INTEGER, source INTEGER NOT NULL,
                   project TEXT NOT NULL DEFAULT ''
                 );
                 CREATE TABLE aliases (alias TEXT PRIMARY KEY, canonical_id TEXT NOT NULL);
                 CREATE TABLE applied (position INTEGER PRIMARY KEY);
                 INSERT INTO nodes (id, kind, attrs, project)
                   VALUES ('old-d', 'decision', '{\"summary\":\"legacy\"}', 'p');
                 INSERT INTO nodes (id, kind, attrs, project) VALUES ('old.rs', 'artifact', NULL, 'p');
                 INSERT INTO edges (from_id, to_id, rel, valid_from, valid_to, source, project)
                   VALUES ('old-d', 'old.rs', 'GOVERNS', 100, NULL, 1, 'p');",
            )
            .unwrap();
        }

        // Opening through the scoped constructor runs both migrations (project already present,
        // tier newly added) without error, and the legacy edge survives, backfilled to EXTRACTED.
        let p = Projector::open(path, "p").unwrap();
        let g = p.subgraph(&["old.rs".to_string()], 2).unwrap();
        assert_eq!(
            edge_tier(&g, REL_GOVERNS, "old.rs"),
            TIER_EXTRACTED,
            "the migrated legacy edge survives and backfills to the EXTRACTED tier"
        );
    }

    #[test]
    fn tier_default_matches_the_extracted_const() {
        // The `SCHEMA` / migration SQL hard-codes the tier column default as the literal 'extracted'
        // (a const cannot be spliced into the SQL literal), and every persisted tier is read back as
        // its literal. Pin all three in lockstep with their consts, so a rename of a const can never
        // silently diverge from the stored value.
        assert_eq!(TIER_EXTRACTED, "extracted");
        assert_eq!(TIER_INFERRED, "inferred");
        assert_eq!(TIER_AMBIGUOUS, "ambiguous");
    }

    // ---- spec 29a criterion 4: the code graph is REBUILDABLE from the log ----

    /// Every user-defined index name on the graph tables, read from `sqlite_master` (the
    /// implicit `sqlite_autoindex_*` primary-key indexes are excluded), so a test can assert an
    /// additive migration created a named index.
    fn index_names(p: &Projector) -> BTreeSet<String> {
        raw_rows(
            p,
            "SELECT name FROM sqlite_master
              WHERE type = 'index' AND name NOT LIKE 'sqlite_%'",
            [],
            |r| r.get(0),
        )
    }

    /// The `EXPLAIN QUERY PLAN` `detail` lines for `sql`, so a test can assert the planner chose a
    /// named index rather than a full table scan.
    fn query_plan(p: &Projector, sql: &str, params: impl rusqlite::Params) -> Vec<String> {
        let conn = p.conn.lock().unwrap();
        let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
        stmt.query_map(params, |r| r.get::<_, String>(3))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    /// Assert some step of `plan`, a [`query_plan`] answer, spells `step`.
    fn assert_plan_uses(plan: &[String], step: &str) {
        assert!(plan.iter().any(|d| d.contains(step)), "{plan:?}");
    }

    #[test]
    fn additive_indexes_exist_and_the_leaf_bound_edge_and_node_seeks_use_them() {
        // Spec 45 criterion 4, re-expressed by spec 119 criterion 2: the additive indexes that
        // keep whole-graph and directed reads sub-linear are present after migration and are
        // actually USED, and the node name-suffix index is gone. This test names NO feature-gated
        // symbol, so it compiles and runs identically on the default and `--no-default-features`
        // lanes. `open` runs the migration, so a fresh db already carries every index.
        let p = Projector::open(":memory:", "test").unwrap();
        let names = index_names(&p);
        for present in [
            "idx_edges_live_rel_from",
            "idx_nodes_leaf",
            "idx_edges_live_to_name_suffix",
        ] {
            assert!(
                names.contains(present),
                "{present} is present; got {names:?}"
            );
        }
        assert!(
            !names.contains("idx_nodes_name_suffix"),
            "the node name-suffix index is retired; got {names:?}"
        );

        // ARE USED (edge expression index): the edge side's two-value binding of a leaf seeks the
        // live-edge target-text index, the expression it embeds being the indexed one.
        let bound = format!(
            "SELECT from_id FROM edges WHERE valid_to IS NULL AND {}",
            edge_binds_leaf_sql("to_id", "?1")
        );
        let plan = query_plan(&p, &bound, ["new"]);
        assert!(
            plan.iter()
                .any(|d| d.contains("USING INDEX idx_edges_live_to_name_suffix")),
            "the two-value edge seek uses idx_edges_live_to_name_suffix; plan was {plan:?}"
        );

        // ARE USED (partial index): the relationship-scoped forward scan
        // (`rel = ? AND valid_to IS NULL`, the shape a directed CALLS traversal walks) is served by
        // the partial live-edge index. The query carries the exact `valid_to IS NULL` term, so
        // SQLite is allowed to pick the partial index over the plain `from_id`/`to_id` indexes.
        let plan = query_plan(
            &p,
            "SELECT to_id FROM edges WHERE rel = 'CALLS' AND valid_to IS NULL",
            [],
        );
        assert!(
            plan.iter().any(|d| d.contains("idx_edges_live_rel_from")),
            "the relationship-scoped forward scan uses the partial idx_edges_live_rel_from; plan was {plan:?}"
        );
    }

    /// Spec 119 criterion 1 (THE LEAF KEYS RESOLUTION): every node-side reader keys a definition
    /// by its leaf, so a nested definition written directly in `a.rs` (`a.rs::T::new`, named
    /// `new`) answers the references `b.rs` folds after it, one assertion per node-side row.
    mod leaf_keys_resolution {
        use super::*;
        use rigger_domain::contextgraph::query::node_leaf;

        /// Run `write` in one committed transaction on `p`'s own connection: the direct writes a
        /// test makes beneath the fold - a nested id planted before any payload carries its path,
        /// a row no fold leaves behind, or a file brought back to an older schema.
        pub(super) fn write_directly(p: &Projector, write: impl FnOnce(&Transaction, &str)) {
            let mut conn = p.conn.lock().unwrap();
            let tx = conn.transaction().unwrap();
            write(&tx, &p.project);
            tx.commit().unwrap();
        }

        /// Write code entity `id` carrying `attrs`.
        pub(super) fn entity(tx: &Transaction, project: &str, id: &str, attrs: &[(&str, &str)]) {
            ensure_node(tx, id, KIND_CODE_ENTITY, attrs, project).unwrap();
        }

        /// Write the function definitions `(id, name)`.
        pub(super) fn define(p: &Projector, defs: &[(&str, &str)]) {
            let defs: Vec<_> = defs
                .iter()
                .map(|(id, name)| (*id, *name, "function"))
                .collect();
            define_kinds(p, &defs);
        }

        /// Write the definitions `(id, name, kind)`.
        pub(super) fn define_kinds(p: &Projector, defs: &[(&str, &str, &str)]) {
            write_directly(p, |tx, project| {
                for (id, name, kind) in defs {
                    let attrs = [("name", *name), ("kind", *kind), ("line", "1")];
                    entity(tx, project, id, &attrs);
                }
            });
        }

        /// Write the live `rel` edge `from -> to` at `tier`, first planting each endpoint
        /// no node holds yet as a nameless node: a file for an id holding no `::`, else a code
        /// entity. The edge is spelled whatever `to` spells, before any fold spells it.
        pub(super) fn edge_directly(p: &Projector, from: &str, to: &str, rel: &str, tier: &str) {
            write_directly(p, |tx, project| {
                for id in [from, to] {
                    if node_row(tx, id, project).unwrap().is_none() {
                        let kind = if id.contains("::") {
                            KIND_CODE_ENTITY
                        } else {
                            KIND_FILE
                        };
                        ensure_node(tx, id, kind, &[], project).unwrap();
                    }
                }
                add_edge(tx, from, to, rel, 0, 4, project, tier).unwrap();
            });
        }

        /// The value the SQL expression `expr` gives node `id`'s row, paired with the value the
        /// in-memory rule `read` it spells gives the same stored node.
        pub(super) fn stored_beside(
            p: &Projector,
            expr: &str,
            id: &str,
            read: fn(&Node) -> Option<&str>,
        ) -> (Option<String>, Option<String>) {
            let sql = format!("SELECT {expr} FROM nodes WHERE id = ?1");
            let stored = raw_rows::<_, Vec<_>>(p, &sql, [id], |r| r.get(0)).remove(0);
            let conn = p.conn.lock().unwrap();
            let node = node_row(&conn, id, &p.project).unwrap().unwrap();
            (stored, read(&node).map(str::to_string))
        }

        /// The id `p.locate(query)` resolves to, insisting it resolves to exactly one.
        pub(super) fn located(p: &Projector, query: &str) -> String {
            match p.locate(query).unwrap() {
                Located::One(site) => site.id,
                other => panic!("{query} resolves to one node; got {other:?}"),
            }
        }

        #[test]
        fn locate_answers_a_bare_query_by_the_leaf_of_a_nested_definition() {
            let p = Projector::open(":memory:", "test").unwrap();
            write_directly(&p, |tx, project| {
                let new = [("name", "new"), ("kind", "function"), ("line", "3")];
                entity(tx, project, "a.rs::T::new", &new);
                let field = [("name", "field"), ("kind", "field"), ("line", "5")];
                entity(tx, project, "a.rs::S.field", &field);
                let op = [("name", "%"), ("kind", "method"), ("line", "7")];
                entity(tx, project, "a.rs::T::%", &op);
            });
            assert_eq!(located(&p, "new"), "a.rs::T::new");
            assert_eq!(located(&p, "field"), "a.rs::S.field");
            assert_eq!(located(&p, "%"), "a.rs::T::%");
        }

        #[test]
        fn the_pinned_leaf_expression_answers_as_node_leaf() {
            let p = Projector::open(":memory:", "test").unwrap();
            let named = [
                ("a.rs::T::new", "new"),
                ("f.ts::A.B", "A.B"),
                ("f.ts::a::b", "a::b"),
                ("x", ""),
            ];
            define(&p, &named);
            let leaves: Vec<(Option<String>, Option<String>)> = named
                .iter()
                .map(|(id, _)| stored_beside(&p, node_leaf_sql(), id, node_leaf))
                .collect();
            let expected = [Some("new"), Some("A.B"), Some("a::b"), None]
                .map(|leaf| leaf.map(str::to_string))
                .map(|leaf| (leaf.clone(), leaf));
            assert_eq!(leaves, expected);
        }

        #[test]
        fn the_leaf_index_serves_the_pinned_leaf_query() {
            let p = Projector::open(":memory:", "test").unwrap();
            assert!(index_names(&p).contains("idx_nodes_leaf"));
            define(&p, &[("a.rs::T::new", "new"), ("b.rs::new", "new")]);
            let pinned = format!(
                "SELECT id FROM nodes WHERE project = 'test' AND {} = 'new'",
                node_leaf_sql()
            );
            assert_plan_uses(&query_plan(&p, &pinned, []), "USING INDEX idx_nodes_leaf");
        }
    }

    /// Spec 119 criterion 14 (THE MEMBER RULE): a file's members are its named code entities by
    /// `query::member_file`, spelled in the store by `member_sql()`, which `members` embeds, so
    /// `subgraph`'s member hops load a file's members one hop on and a member its file, with no
    /// file `CONTAINS` written.
    mod member_rule {
        use super::leaf_keys_resolution::{
            define, edge_directly, entity, stored_beside, write_directly,
        };
        use super::*;
        use crate::test_support::{
            file_half, fold_events, hold, BARE_PARENT, FILE_MATE, FLAT_LEAF_ID, LEAF_PLACEHOLDER,
            MAKE_TARGET, NESTED_METHOD, RANGE_ARTIFACT,
        };
        use rigger_domain::contextgraph::query::{member_file, path_leaf};

        /// Plant, by direct writes with no edge: [`NESTED_METHOD`]'s file node, that method and
        /// [`FILE_MATE`] named by their leaves (only an unkeyed assertion holds each), a named
        /// definition whose id holds no `::` (that leaf alone), [`LEAF_PLACEHOLDER`] and
        /// [`BARE_PARENT`] nameless, [`RANGE_ARTIFACT`] an `artifact` carrying its leaf, and
        /// [`FLAT_LEAF_ID`] held by a decision.
        fn plant() -> Projector {
            let p = fold_events([]);
            let named = [NESTED_METHOD, FILE_MATE, RANGE_ARTIFACT].map(|id| (id, path_leaf(id)));
            define(&p, &named);
            let fileless = path_leaf(FILE_MATE);
            define(&p, &[(fileless, fileless)]);
            write_directly(&p, |tx, project| {
                let file = file_half(NESTED_METHOD);
                ensure_node(tx, file, KIND_FILE, &[], project).unwrap();
                for bare in [LEAF_PLACEHOLDER, BARE_PARENT] {
                    entity(tx, project, bare, &[]);
                }
                tx.execute(
                    "UPDATE nodes SET kind = ?1 WHERE id = ?2 AND project = ?3",
                    params![KIND_ARTIFACT, RANGE_ARTIFACT, project],
                )
                .unwrap();
            });
            hold(&p, 1, &[FLAT_LEAF_ID]);
            p
        }

        /// The ids of the nodes `p.subgraph` loads from `seed` within `depth` hops, sorted.
        fn reach(p: &Projector, seed: &str, depth: i64) -> Vec<String> {
            let g = p.subgraph(&[seed.to_string()], depth).unwrap();
            let mut ids: Vec<String> = g.nodes.into_iter().map(|n| n.id).collect();
            ids.sort();
            ids
        }

        #[test]
        fn a_file_seed_loads_its_members_one_hop_on_and_no_nameless_node() {
            let p = plant();
            let file = file_half(NESTED_METHOD);
            assert_eq!(reach(&p, file, 0), [file]);
            assert_eq!(reach(&p, file, 1), [file, NESTED_METHOD, FILE_MATE]);
        }

        #[test]
        fn a_member_seed_loads_its_file_one_hop_on_and_its_file_mates_the_next() {
            let p = plant();
            let file = file_half(NESTED_METHOD);
            assert_eq!(reach(&p, NESTED_METHOD, 0), [NESTED_METHOD]);
            assert_eq!(reach(&p, NESTED_METHOD, 1), [file, NESTED_METHOD]);
            assert_eq!(
                reach(&p, NESTED_METHOD, 2),
                [file, NESTED_METHOD, FILE_MATE]
            );
        }

        #[test]
        fn a_members_neighbour_sits_one_hop_past_the_member() {
            let p = plant();
            let (file, outside) = (file_half(NESTED_METHOD), file_half(MAKE_TARGET));
            edge_directly(&p, FILE_MATE, outside, REL_CALLS, TIER_INFERRED);
            assert_eq!(reach(&p, file, 1), [file, NESTED_METHOD, FILE_MATE]);
            assert_eq!(
                reach(&p, file, 2),
                [outside, file, NESTED_METHOD, FILE_MATE]
            );
        }

        #[test]
        fn another_projects_members_stay_out_of_the_reach() {
            let p = plant();
            let other = format!("{}-other", p.project);
            for id in [NESTED_METHOD, FILE_MATE] {
                write_directly(&p, |tx, _| {
                    tx.execute(
                        "UPDATE nodes SET project = ?1 WHERE id = ?2",
                        params![other, id],
                    )
                    .unwrap();
                });
            }
            write_directly(&p, |tx, project| {
                entity(tx, project, NESTED_METHOD, &[]);
            });
            let file = file_half(NESTED_METHOD);
            assert_eq!(reach(&p, file, 1), [file]);
            assert_eq!(reach(&p, NESTED_METHOD, 1), [NESTED_METHOD]);
        }

        #[test]
        fn a_bare_parent_seed_loads_no_file() {
            let p = plant();
            assert_eq!(reach(&p, BARE_PARENT, 2), [BARE_PARENT]);
        }

        #[test]
        fn the_member_expression_answers_as_member_file() {
            let p = plant();
            let ids = [
                NESTED_METHOD,
                LEAF_PLACEHOLDER,
                BARE_PARENT,
                FLAT_LEAF_ID,
                RANGE_ARTIFACT,
                path_leaf(FILE_MATE),
            ];
            let files: Vec<(Option<String>, Option<String>)> = ids
                .into_iter()
                .map(|id| stored_beside(&p, &member_sql(), id, member_file))
                .collect();
            let file = Some(file_half(NESTED_METHOD).to_string());
            let expected = [file, None, None, None, None, None].map(|f| (f.clone(), f));
            assert_eq!(files, expected);
        }

        #[test]
        fn the_id_range_holds_exactly_the_ids_whose_file_half_is_the_file() {
            let p = plant();
            let file = file_half(NESTED_METHOD);
            let sql = format!("SELECT {} FROM nodes LIMIT 1", id_range_sql("?1", "?2"));
            let below = format!("{file}:");
            let empty_rest = format!("{file}::");
            let above = format!("{file}:;");
            let held: Vec<bool> = [below.as_str(), empty_rest.as_str(), NESTED_METHOD, &above]
                .into_iter()
                .map(|id| raw_rows::<_, Vec<_>>(&p, &sql, [id, file], |r| r.get(0)).remove(0))
                .collect();
            assert_eq!(held, [false, true, true, false]);
        }
    }

    /// Spec 119 criterion 7 (A BARE SHOW FINDS A LEAF OR PATH) over the record graph: a record
    /// whose id holds `::` answers its own id where no definition does, over hand-built events.
    mod a_bare_show_finds_a_leaf_or_path {
        use super::leaf_keys_resolution::located;
        use super::*;
        use crate::test_support::hold;

        /// Once `then` folds at position 1, each `(query, id)` of `shown` shows the one node `id`.
        fn shows(then: impl FnOnce(&Projector, u64), shown: &[(&str, &str)]) {
            let p = Projector::open(":memory:", "test").unwrap();
            then(&p, 1);
            for (query, id) in shown {
                assert_eq!(located(&p, query), *id, "{query}");
            }
        }

        crate::test_cases! {
            a_flat_id_a_decision_is_named_by_answers_that_decision: shows(
                |p, pos| apply_decision(p, pos, "a.rs::new", "flat", &[], ""),
                &[("a.rs::new", "a.rs::new")],
            );
            an_artifact_spelling_a_path_answers_where_no_definition_does: shows(
                |p, pos| hold(p, pos, &["T::new"]),
                &[("T::new", "T::new")],
            );
        }
    }

    /// Spec 119 criterion 2 (THE EDGE SIDE BINDS THE LEAF): every edge-side reader binds its
    /// seed's leaf and matches an edge's target text against the leaf and the sigil-prefixed
    /// leaf, over a nested definition `a.rs::T::new` written directly and `b.rs`'s references to
    /// `new`, each spelled `b.rs::new` and `b.rs::%new`.
    mod edge_side_binds_the_leaf {
        use super::leaf_keys_resolution::{define, edge_directly, entity, write_directly};
        use super::*;
        use crate::test_support::{
            call_frontiers, call_node_ids, call_walk, referenced_not_called_ids,
        };

        /// The callers walk from `seed`, a definition of leaf `leaf` written directly, reaches the
        /// caller `caller` whose call targets the nameless `target`, as an unambiguous caller.
        fn the_callers_walk_reaches(seed: (&str, &str), caller: &str, target: &str) {
            let p = Projector::open(":memory:", "test").unwrap();
            define(&p, &[seed]);
            edge_directly(&p, caller, target, REL_CALLS, TIER_INFERRED);
            let up = call_walk(&records_graph(&p), seed.0, Direction::Up);
            assert_eq!(call_node_ids(&up), vec![seed.0, caller]);
            assert_eq!(call_frontiers(&up), Vec::new());
        }

        crate::test_cases! {
            the_callers_walk_from_a_nested_definition_reaches_a_caller_through_its_bare_leaf:
                the_callers_walk_reaches(("a.rs::T::new", "new"), "b.rs::run", "b.rs::new");
            the_callers_walk_from_a_nested_definition_reaches_a_caller_through_its_sigiled_leaf:
                the_callers_walk_reaches(("a.rs::T::new", "new"), "b.rs::run", "b.rs::%new");
            a_csharp_operator_percent_seed_reaches_its_double_percent_caller:
                the_callers_walk_reaches(("a.cs::T::%", "%"), "b.cs::run", "b.cs::%%");
        }

        #[test]
        fn referenced_not_called_counts_a_reference_through_the_leaf_its_sigil_and_a_nested_landing(
        ) {
            let p = Projector::open(":memory:", "test").unwrap();
            let nested = [("z.rs::T::new", "new"), ("w.rs::T::new", "new")];
            define(&p, &[("a.rs::T::new", "new")]);
            define(&p, &nested);
            for (file, target) in [
                ("x.rs", "x.rs::new"),
                ("y.rs", "y.rs::%new"),
                ("z.rs", "z.rs::T::new"),
                ("u.rs", "u.rs::new"),
                ("v.rs", "v.rs::%new"),
                ("w.rs", "w.rs::T::new"),
                ("o.rs", "o.rs::other"),
            ] {
                edge_directly(&p, file, target, REL_REFERENCES, TIER_INFERRED);
            }
            // `u.rs`, `v.rs` and `w.rs` also call `new` from within, through each spelling.
            for (caller, target) in [
                ("u.rs::run", "u.rs::new"),
                ("v.rs::run", "v.rs::%new"),
                ("w.rs::run", "w.rs::T::new"),
            ] {
                edge_directly(&p, caller, target, REL_CALLS, TIER_INFERRED);
            }
            assert_eq!(
                referenced_not_called_ids(&records_graph(&p), "a.rs::T::new"),
                vec!["x.rs", "y.rs", "z.rs"]
            );
        }

        /// A leg of the referenced-but-not-called read whose edge's source cannot be read fails
        /// the UP walk with that read's error, at a depth that walks no caller: `rel`'s edge from
        /// `b.rs` onto `b.rs::new`, beside the definition `a.rs::T::new`, its source made a blob.
        fn an_unreadable_leg_fails_the_up_walk(rel: &str) {
            let p = Projector::open(":memory:", "test").unwrap();
            define(&p, &[("a.rs::T::new", "new")]);
            edge_directly(&p, "b.rs", "b.rs::new", rel, TIER_INFERRED);
            write_directly(&p, |tx, _| {
                tx.execute(
                    "UPDATE edges SET from_id = x'00' WHERE to_id = 'b.rs::new'",
                    [],
                )
                .unwrap();
            });
            assert_eq!(
                p.calls(
                    &["a.rs::T::new".to_string()],
                    Direction::Up,
                    0,
                    TIER_INFERRED
                )
                .map(drop)
                .map_err(|e| e.0),
                Err("Invalid column type Blob at index: 0, name: from_id".to_string())
            );
        }

        crate::test_cases! {
            an_unreadable_referencing_leg_fails_the_up_walk:
                an_unreadable_leg_fails_the_up_walk(REL_REFERENCES);
            an_unreadable_calling_leg_fails_the_up_walk:
                an_unreadable_leg_fails_the_up_walk(REL_CALLS);
        }

        #[test]
        fn a_seed_with_no_leaf_names_nothing_referenced() {
            let p = Projector::open(":memory:", "test").unwrap();
            write_directly(&p, |tx, project| entity(tx, project, "x", &[]));
            edge_directly(&p, "f.rs", "f.rs::x", REL_REFERENCES, TIER_AMBIGUOUS);
            assert_eq!(
                referenced_not_called_ids(&records_graph(&p), "x"),
                Vec::<String>::new()
            );
        }
    }
}
