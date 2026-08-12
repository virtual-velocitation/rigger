//! SQLite-backed EventStore. A single connection behind a mutex serializes
//! writes, so concurrent appenders queue instead of deadlocking on the
//! lock-upgrade (SQLITE_BUSY) class. Per-stream revisions and a `UNIQUE(stream,
//! revision)` index give optimistic concurrency; `$all` is `ORDER BY position`.

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use super::{
    Appended, ContentIdentity, Direction, Error, Event, EventStore, ExpectedRevision, Filter,
    Position, Revision, Subscription, GUARD_DEGRADED_NO_INDEX, GUARD_DEGRADED_UNDETERMINED,
    META_GUARD_DEGRADED, NO_STREAM,
};

const SCHEMA: &str = "
PRAGMA journal_mode=WAL;
PRAGMA busy_timeout=5000;
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
";

const COLS: &str = "position, stream, type, id, data, meta, valid_from, recorded_at, revision";

/// The stem every content-key index name is built on. The index's DEFINITION carries the
/// configured metadata key (that is what `json_extract` reads), so its NAME carries it
/// too: a second policy configured with a different key gets its OWN artifact instead of
/// silently inheriting - and degrading against - the first policy's. See
/// [`Store::content_key_index_name`].
const CONTENT_KEY_INDEX_STEM: &str = "idx_events_content_key";

/// A hard ceiling on the number of index seeks ONE latest-generation probe may spend.
///
/// The walk below advances one CONTENT GENERATION per step, so this bounds the probe by
/// the number of generations a single subject has recorded, never by the size of the log
/// or of that subject's history in events. It exists so "bounded" is a property of the
/// CODE rather than of the data: a subject whose recorded history somehow exceeds it
/// leaves the probe undetermined, and an undetermined probe does NOT suppress - it
/// appends, which is the fail-safe direction.
const LATEST_GENERATION_STEPS: usize = 1024;

/// Store is the SQLite-backed EventStore. The connection is shared (Arc) so a
/// subscription's polling thread reads the same database the writers append to.
pub struct Store {
    conn: Arc<Mutex<Connection>>,
    /// The configured content-identity policy and the SQL rendered from it, or `None`
    /// for a store with no guard (which appends everything through).
    guard: Option<Guard>,
}

/// The configured guard: the policy, the index its probes require, the probe
/// statements, and whether that index is THERE - all rendered from the policy ONCE, at
/// configuration time, so the text the query planner sees can never drift from the
/// indexed expression (they are built from the same key expression).
///
/// The readiness latch lives HERE, beside the policy it is a fact about, and not on the
/// [`Store`]. A latch on the store is a second field that can disagree with the first:
/// reconfiguring a handle replaces the policy, and a latch left behind then reports
/// "the index is ready" about an index that policy never built - so the guard believes
/// it is seeking an index while every probe walks the table, under the append's write
/// lock. Held on the guard, the latch is created with the policy and dies with it:
/// there is no second value to reset and nothing to keep in step.
struct Guard {
    identity: ContentIdentity,
    /// The name of the index the probes seek, derived from the configured metadata key.
    index_name: String,
    /// The exact `CREATE INDEX` text this policy requires, spelled as SQLite STORES it
    /// in `sqlite_master` (no `IF NOT EXISTS`), so a committed definition can be
    /// compared to it verbatim.
    index_ddl: String,
    /// `SELECT EXISTS(...)`: is this exact content key already recorded, on an event
    /// of a covered type?
    recorded_sql: String,
    /// ONE step of the generation walk: the first covered content key at or after a lower
    /// bound and below an upper bound, TOGETHER WITH when that key was last recorded.
    /// Both halves come from one index seek, and from ONE type test - so the type gate
    /// cannot be present for the candidate and absent for its date.
    step_sql: String,
    /// Whether THIS policy's content-key index is COMMITTED, with the definition this
    /// policy renders. Latched true only after that definition has been read back out
    /// of `sqlite_master` in its own statement, so a build that was rolled back can
    /// never leave the handle believing the index is there: an unlatched guard
    /// re-checks (and re-attempts) on the next append. While it is false the guard
    /// suppresses NOTHING - see [`Store::redundant_flags`].
    index_ready: AtomicBool,
}

/// Whether the guard is in a position to JUDGE one particular append - decided BEFORE
/// the append's write transaction opens, because settling it is what may have to build
/// an index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Readiness {
    /// Nothing to judge: no policy configured, or nothing in this batch that the
    /// configured policy covers. Not a degradation - the guard was not asked.
    Idle,
    /// The content-key index this policy needs is committed, so every probe is a seek.
    Indexed,
    /// The guard COVERS something in this batch and has no usable index. It suppresses
    /// nothing (a probe without the index is a table walk under the write lock, which
    /// costs more than the duplication it removes), and it says so: see
    /// [`GUARD_DEGRADED_NO_INDEX`].
    Unindexed,
}

/// What a bounded latest-generation walk ESTABLISHED about one subject.
///
/// `Absent` and `Undetermined` are deliberately different values even though neither
/// suppresses. `Absent` is an answer - this subject has recorded no generation - and it
/// is the normal state of every subject the log has not seen. `Undetermined` is the
/// walk admitting it ran out of budget before it could answer, which is the guard
/// failing to do its job, and a guard that fails silently is the failure mode this
/// layer has already been rebuilt for once. Keeping them apart is what lets the append
/// record the second and stay quiet about the first.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Latest {
    /// The generation this subject is currently at.
    At(String),
    /// The subject has no recorded generation on this stream.
    Absent,
    /// The walk exceeded its step budget, so the current generation is not known.
    Undetermined,
}

/// One append's suppression verdicts, and what the guard COULD NOT DO while taking them.
///
/// The two travel together because they are decided together and are both facts about
/// the same append: the flags say which events the store may skip, and `degraded` says
/// whether those flags were taken by a guard that was actually able to judge. A caller
/// cannot get one without the other, which is what keeps a degradation from being
/// dropped on the floor between the probe and the write.
struct Verdicts {
    /// One verdict per handed event, in input order.
    redundant: Vec<bool>,
    /// Why the guard was not judging, when it was not - one of
    /// [`GUARD_DEGRADED_NO_INDEX`] / [`GUARD_DEGRADED_UNDETERMINED`], stamped under
    /// [`META_GUARD_DEGRADED`] onto the covered events this append writes.
    degraded: Option<&'static str>,
}

impl Store {
    /// Open (creating if needed) the store at path. Use ":memory:" in tests.
    pub fn open(path: &str) -> Result<Self, Error> {
        let conn = Connection::open(path).map_err(be)?;
        conn.execute_batch(SCHEMA).map_err(be)?;
        Ok(Store {
            conn: Arc::new(Mutex::new(conn)),
            guard: None,
        })
    }

    /// Configure the content-identity guard: appending an event of a covered type
    /// whose content key is already recorded AS ITS SUBJECT'S LATEST GENERATION
    /// becomes a storage no-op.
    ///
    /// This is defense in depth UNDER the ingest sink's own project-scoped dedup, so a
    /// regression above the port can never re-bloat the log, and it applies the SAME
    /// latest-per-subject test the sink does - never a wider ever-recorded one. An
    /// ever-recorded test would swallow a REVERT: content returned to a generation the
    /// subject has since moved past is a CHANGE, its re-append is not redundant, and
    /// suppressing it would strand the projection on the superseded generation with no
    /// recovery, because re-folding the log would replay the same suppression.
    ///
    /// The recorded state this tests is THE LOG'S, asked of the content keys the log
    /// ALREADY carries: no new event type, no new metadata, no backfill, and a fresh
    /// process's first append gets the same answer as a long-lived one's thousandth.
    /// (An in-memory seen-set would answer neither: the duplication this exists to
    /// stop is cross-PROCESS.)
    ///
    /// Configuring touches no connection and issues no statement - it is a pure value
    /// change - so a read-only or maintenance path may construct through here without
    /// ever writing the store it opened. The index the probes seek is created lazily, on
    /// the first append that could actually be suppressed, and until it is COMMITTED the
    /// guard suppresses nothing: every probe is an index seek by construction, and a
    /// guard that fell back to walking the table would cost more, under the write lock,
    /// than the duplication it removes.
    pub fn with_content_identity(mut self, identity: ContentIdentity) -> Self {
        let key = key_expr(identity.meta_key());
        let types = type_list(identity.types());
        let index_name = Self::content_key_index_name(identity.meta_key());
        self.guard = Some(Guard {
            index_ddl: Self::content_key_index_ddl(&index_name, identity.meta_key()),
            index_name,
            recorded_sql: format!(
                "SELECT EXISTS(SELECT 1 FROM events \
                 WHERE {key} = ?1 AND stream = ?2 AND type IN ({types}))"
            ),
            step_sql: format!(
                "SELECT {key}, MAX(position) FROM events \
                 WHERE stream = ?1 AND {key} >= ?2 AND {key} < ?3 AND type IN ({types}) \
                 GROUP BY {key} ORDER BY {key} ASC LIMIT 1"
            ),
            identity,
            // A NEW policy starts with a NEW latch, unlatched. This is the whole
            // reconfiguration story: there is no flag left over from the policy this
            // one replaces, so nothing has to be reset and nothing can be forgotten.
            index_ready: AtomicBool::new(false),
        });
        self
    }

    /// The name of the content-key index a policy configured with `meta_key` requires.
    ///
    /// The name CARRIES the metadata key, because the index's definition does: the
    /// indexed expression is `json_extract(meta, '$."<meta_key>"')`, so an index built
    /// for one policy answers no question at all for a policy configured with another
    /// key. A fixed name would let the second policy silently inherit the first's
    /// artifact - every probe then falls off the index and walks the table, which is
    /// exactly the silent degradation this guard exists to end. A digest rides along so
    /// two keys that sanitize to the same identifier still get distinct artifacts.
    fn content_key_index_name(meta_key: &str) -> String {
        let sanitized: String = meta_key
            .chars()
            .take(24)
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        format!(
            "{CONTENT_KEY_INDEX_STEM}_{sanitized}_{:016x}",
            fnv1a64(meta_key)
        )
    }

    /// The DDL for the content-key index `name` under a policy keyed on `meta_key`.
    ///
    /// WHAT THIS ARTIFACT COSTS, stated because this store exists to BOUND the log and
    /// a guard that quietly grows it by a third has not bounded anything. The index
    /// holds one entry per event of a covered type, carrying that event's content key -
    /// a path-and-hash string, not a small integer - so it measures a comparable
    /// fraction of the rows it indexes: on a log of the scale this spec was written for
    /// it is a hundreds-of-megabytes artifact, roughly a third of the store's size at
    /// the moment the guard is switched on. It is worth that only against what it
    /// removes: the duplication it stops is UNBOUNDED (the whole derived index
    /// re-appended per run, measured at 39.5x), so a fixed proportional cost buys a
    /// growth rate. On a log that never receives a duplicate append it is pure
    /// overhead, which is why it is built LAZILY - a store nobody appends covered
    /// events to never pays for it at all - and why RECLAIMING it is a named
    /// responsibility rather than an accident: see
    /// [`Store::ensure_content_key_index`], which sweeps every content-key index that
    /// is not the configured policy's.
    ///
    /// Spelled WITHOUT `IF NOT EXISTS` on purpose: SQLite strips that clause when it
    /// stores a definition in `sqlite_master`, so omitting it makes this text exactly
    /// what a committed index reads back as - which is what lets
    /// [`Store::ensure_content_key_index`] compare the COMMITTED DEFINITION rather than
    /// trust a name.
    ///
    /// `(stream, <content key>, position)` in that order, because that is the shape the
    /// probes ask for: the stream is an equality (the project boundary), the content key
    /// is an equality for the recorded probe and a half-open RANGE for the walk's step,
    /// and the position rides along so a key's last recording is read off the index
    /// itself rather than out of the table.
    fn content_key_index_ddl(name: &str, meta_key: &str) -> String {
        format!(
            "CREATE INDEX {name} ON events(stream, {}, position)",
            key_expr(meta_key)
        )
    }

    /// Whether the guard's probes have the index they require, building it if they do
    /// not. Returns `false` when the store must NOT suppress.
    ///
    /// Three properties, each of them load-bearing:
    ///
    /// 1. **The gate is the committed DEFINITION, never a name.** An index built for a
    ///    different metadata key, or a half-written one, satisfies a name check and then
    ///    degrades every probe into a table walk. So the definition is read back out of
    ///    `sqlite_master` and compared verbatim, and a mismatch is REBUILT (dropped and
    ///    recreated) rather than accepted.
    /// 2. **Readiness is only ever latched from a COMMITTED read.** The flag is set after
    ///    the build's transaction has ended and the definition has been re-read - never
    ///    on the way in - so a build that rolls back (a lock timeout, a full disk) leaves
    ///    the handle knowing it has no index instead of believing forever that it has one.
    /// 3. **The build takes its OWN transaction, before the append's.** Creating this
    ///    index over an established log is a large write; running it inside the append's
    ///    `BEGIN IMMEDIATE` would add its whole duration to a window every other process
    ///    is queued behind, on top of the append itself. Its own transaction keeps the
    ///    append's write window the size of the append.
    /// 4. **The build is IDEMPOTENT UNDER THE LOCK, and it reclaims what it replaces.**
    ///    Everything above this line is decided outside the write lock, so on a cold
    ///    store every handle that starts before the first one commits reaches the build
    ///    with the same verdict. Re-reading the committed definition once the lock is
    ///    held turns k racing rebuilds of a multi-hundred-megabyte artifact into one
    ///    build and k-1 no-ops - see [`Store::build_content_key_index`], which is also
    ///    where an index left behind by a policy that is no longer configured is
    ///    dropped.
    ///
    /// It is reached only from an append that carries a suppressible event, so a store
    /// that is only ever read, or only ever written with uncovered types, never builds it.
    fn ensure_content_key_index(&self, conn: &mut Connection, guard: &Guard) -> bool {
        if guard.index_ready.load(Ordering::SeqCst) {
            return true;
        }
        if committed_index_ddl(conn, &guard.index_name).as_deref() == Some(guard.index_ddl.as_str())
            && stale_content_key_indexes(conn, &guard.index_name).is_empty()
        {
            guard.index_ready.store(true, Ordering::SeqCst);
            return true;
        }
        self.build_content_key_index(conn, guard);
        let ready = committed_index_ddl(conn, &guard.index_name).as_deref()
            == Some(guard.index_ddl.as_str());
        if ready {
            guard.index_ready.store(true, Ordering::SeqCst);
        }
        ready
    }

    /// Build this policy's content-key index, and drop every content-key index that is
    /// not it, in ONE write transaction.
    ///
    /// THE FIRST THING IT DOES UNDER THE LOCK IS LOOK AGAIN. The decision to build was
    /// taken outside the write lock, which is the only place it could have been taken -
    /// and on a cold store that means every handle that starts before the first builder
    /// COMMITS arrives here having seen no index. Without this re-read each of them
    /// drops and recreates the artifact the previous one just committed, serially,
    /// holding the exclusive write lock for the whole build: with four cold handles that
    /// is four builds and seconds of lock against a five-second busy timeout, and every
    /// bystander process appending an ordinary event in that window - a worker's
    /// self-report, a courier's death record - fails with a lock error rather than
    /// waiting. One re-read collapses that to a single build; the others find their work
    /// already done and commit nothing. This is the SUCCESS path, not a misuse path: it
    /// is what a cold store with more than one appender does.
    ///
    /// THE SWEEP is the same transaction's other half. The index's NAME carries the
    /// policy's metadata key, so a store reconfigured onto a different key mints a
    /// different artifact - and the previous one would otherwise stay committed forever,
    /// answering no question anyone asks while still being maintained on every single
    /// insert. Whoever builds the live index is the one process that knows which
    /// artifacts are dead, so reclaiming them is its job and nobody else's.
    ///
    /// The sweep rests on ONE policy per store at a time, which is a property of the
    /// design and not a convention: the metadata key derived facts carry is a code-owned
    /// constant minted in one place, so "this project's content keys" is a single answer
    /// and a second, differently-keyed policy over the same database is not a
    /// configuration this system can produce. If a future composition root produced one
    /// anyway, the two would reclaim each other's artifact and their probes would fall
    /// back to table walks - a cost, in the same fail-safe direction as everything else
    /// here, never a dropped fact.
    ///
    /// The build's own `Result` is deliberately NOT the verdict - a statement that ran is
    /// not an index, and a commit can still fail - so it is discarded here and the
    /// caller reads the committed state instead.
    fn build_content_key_index(&self, conn: &mut Connection, guard: &Guard) {
        let _ = (|| -> rusqlite::Result<()> {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let mut ddl = String::new();
            for name in stale_content_key_indexes(&tx, &guard.index_name) {
                ddl.push_str(&format!("DROP INDEX IF EXISTS {name};\n"));
            }
            if committed_index_ddl(&tx, &guard.index_name).as_deref()
                != Some(guard.index_ddl.as_str())
            {
                ddl.push_str(&format!(
                    "DROP INDEX IF EXISTS {};\n{};",
                    guard.index_name, guard.index_ddl
                ));
            }
            if ddl.is_empty() {
                // Another handle built it while this one queued for the lock. Nothing to
                // write, so nothing is written: the transaction ends having touched no
                // page, and the artifact is not rebuilt.
                return Ok(());
            }
            tx.execute_batch(&ddl)?;
            tx.commit()
        })();
    }

    /// Whether `events` carries anything this store's guard could suppress: an event of a
    /// covered type that actually carries a content key. A pure in-memory test over the
    /// batch - it issues no statement - so an append with nothing suppressible in it never
    /// touches the index at all.
    fn has_suppressible(guard: &Guard, events: &[Event]) -> bool {
        events.iter().any(|e| {
            guard.identity.covers(&e.type_) && e.meta.contains_key(guard.identity.meta_key())
        })
    }

    /// Which of `events` are redundant - one verdict per event, in input order.
    ///
    /// Every verdict is taken against the state the log was in when the append STARTED,
    /// which is why they are all decided before any row is inserted: an event written
    /// earlier in the same batch would otherwise become the subject's "latest recorded
    /// generation" and make its own siblings look redundant, so a reverted file would
    /// re-append its first event and have the rest swallowed - the exact half-landed
    /// batch this guard exists to prevent.
    ///
    /// The order of the test is load-bearing. TYPE first: an event of any other type
    /// never reaches the key comparison, so no domain event can be dropped here however
    /// its metadata is spelled. Then the WHOLE key, split by the configured policy: a
    /// key that names no generation is passed over and appends. Only then the probes.
    fn redundant_flags(
        &self,
        tx: &rusqlite::Transaction<'_>,
        stream: &str,
        events: &[Event],
        readiness: Readiness,
    ) -> Result<Verdicts, Error> {
        let judged_nothing = |degraded| Verdicts {
            redundant: vec![false; events.len()],
            degraded,
        };
        let Some(guard) = &self.guard else {
            return Ok(judged_nothing(None));
        };
        match readiness {
            Readiness::Idle => return Ok(judged_nothing(None)),
            // NO USABLE INDEX, NO SUPPRESSION. Without it every probe becomes a walk of
            // the table, and a guard that costs an unbounded walk per append is worse
            // than the duplication it removes - so the store appends, which is the
            // fail-safe direction this whole layer is built on (it can only ever write
            // MORE, never drop). It is also RECORDED: the events this append writes
            // carry the reason, because a defense that has switched itself off in
            // silence is one nobody discovers until the log is huge again.
            Readiness::Unindexed => return Ok(judged_nothing(Some(GUARD_DEGRADED_NO_INDEX))),
            Readiness::Indexed => {}
        }
        // One latest-generation lookup per DISTINCT subject in the batch, not per event:
        // a file's whole batch shares one subject.
        let mut current: std::collections::HashMap<String, Latest> =
            std::collections::HashMap::new();
        let mut degraded = None;
        let mut redundant = Vec::with_capacity(events.len());
        for event in events {
            redundant.push(self.is_redundant(
                tx,
                guard,
                stream,
                event,
                &mut current,
                &mut degraded,
            )?);
        }
        Ok(Verdicts {
            redundant,
            degraded,
        })
    }

    /// Whether appending `event` would be redundant: its type carries content identity,
    /// its content key is already recorded, and that key is still ITS SUBJECT'S LATEST
    /// recorded generation. `current` memoizes the latest generation per subject for the
    /// duration of one append.
    fn is_redundant(
        &self,
        tx: &rusqlite::Transaction<'_>,
        guard: &Guard,
        stream: &str,
        event: &Event,
        current: &mut std::collections::HashMap<String, Latest>,
        degraded: &mut Option<&'static str>,
    ) -> Result<bool, Error> {
        if !guard.identity.covers(&event.type_) {
            return Ok(false);
        }
        let Some(key) = event.meta.get(guard.identity.meta_key()) else {
            return Ok(false);
        };
        let Some((subject, generation)) = guard.identity.subject_of(key) else {
            return Ok(false);
        };

        let recorded: i64 = tx
            .query_row(&guard.recorded_sql, params![key, stream], |r| r.get(0))
            .map_err(be)?;
        if recorded == 0 {
            return Ok(false);
        }
        if !current.contains_key(subject) {
            let latest = self.latest_generation(tx, guard, stream, subject)?;
            current.insert(subject.to_string(), latest);
        }
        Ok(match current.get(subject) {
            Some(Latest::At(latest)) => latest == generation,
            Some(Latest::Undetermined) => {
                // The walk could not establish this subject's current generation, so
                // nothing is suppressed against it - and the append RECORDS that it was
                // written by a guard which was not judging.
                *degraded = Some(GUARD_DEGRADED_UNDETERMINED);
                false
            }
            _ => false,
        })
    }

    /// The content generation a subject is CURRENTLY at: the one named by the
    /// latest-recorded covered key whose subject is exactly `subject`, ON THE TARGET
    /// STREAM.
    ///
    /// Stream scoping is what makes the guard project-scoped by construction. One
    /// backend can hold many projects (the namespacing decorator gives each its own
    /// stream prefix), and a content key names a RELATIVE path, so two projects that
    /// share a file path with identical content mint the IDENTICAL key. An unscoped
    /// probe would read the second project's genuinely-new fact as already recorded and
    /// drop it - a non-redundant append silently lost.
    ///
    /// It WALKS GENERATIONS, NOT EVENTS. Every key naming this subject begins with the
    /// subject, so the subject's whole history is one half-open range on the content-key
    /// index - but that range is the subject's every event of every generation (one
    /// file's own range on this project's log measures in the hundreds of thousands of
    /// rows, and it grows with every content change), and ordering it by position needs
    /// all of it. So the range is never ordered. It is stepped:
    ///
    /// 1. seek the FIRST covered key at or after a lower bound, and read off the same
    ///    seek when that key was last recorded (`GROUP BY` on the indexed expression);
    /// 2. ask the POLICY to split it - which tells us its subject and its generation;
    /// 3. move the lower bound past that key's WHOLE generation and repeat.
    ///
    /// The cost is therefore ONE index seek per recorded GENERATION of this one subject -
    /// the same asymptote the deduplicated log itself has, since a new generation is
    /// exactly what a genuine content change appends - and [`LATEST_GENERATION_STEPS`]
    /// caps even that. A batch's thousands of sibling keys cost one step between them,
    /// not one each.
    ///
    /// A generation is DATED BY ITS LEADING KEY, which is what makes the walk possible at
    /// all: dating it by the greatest position over all of its keys would have to touch
    /// every one of them, which is the unbounded range this walk exists to avoid. That
    /// dating is exact for every batch this system records, because the only writer of a
    /// PARTIAL generation is this very guard - it writes the new events of a batch whose
    /// siblings are already recorded - and it does that ONLY while that generation is
    /// already the subject's latest, which no dating can change. A generation the file
    /// has moved past is never appended partially: the guard suppresses none of it.
    ///
    /// Stepping past a generation uses the byte RANGES the POLICY located the parts at,
    /// never a separator this module knows: [`ContentIdentity::split_of`] answers WHERE
    /// in the key each part lies, so the span to skip is arithmetic on those offsets
    /// (see [`generation_span`], which also explains why the skip has to reach one
    /// character PAST the generation to be sound). The store still parses no key format
    /// of its own.
    ///
    /// Candidates are filtered by asking the policy for each one's subject, because a
    /// prefix range is a superset: a file whose path itself contains the generation
    /// separator (`vendor/pkg@1.2.3/a.rs` beside a file named `vendor/pkg`) sits inside
    /// the shorter subject's range while belonging to a different subject entirely, and
    /// letting it answer would let one file's generation retire another's. A foreign
    /// subject is skipped WHOLE for the same reason a generation is: by its own range.
    ///
    /// Answers [`Latest::Absent`] for a subject with no recorded generation and
    /// [`Latest::Undetermined`] for a walk that ran out of steps. Neither suppresses -
    /// but only the second is a degradation, and telling them apart is what lets the
    /// append record it.
    fn latest_generation(
        &self,
        tx: &rusqlite::Transaction<'_>,
        guard: &Guard,
        stream: &str,
        subject: &str,
    ) -> Result<Latest, Error> {
        self.latest_generation_within(tx, guard, stream, subject, LATEST_GENERATION_STEPS)
    }

    /// [`Store::latest_generation`] with the step budget given explicitly, so the bound
    /// itself is drivable: a walk handed fewer steps than the subject has generations
    /// must answer [`Latest::Undetermined`], never the best generation it happened to
    /// reach.
    fn latest_generation_within(
        &self,
        tx: &rusqlite::Transaction<'_>,
        guard: &Guard,
        stream: &str,
        subject: &str,
        steps: usize,
    ) -> Result<Latest, Error> {
        let end = prefix_upper_bound(subject);
        let mut step = tx.prepare_cached(&guard.step_sql).map_err(be)?;
        let mut lo = subject.to_string();
        let mut latest: Option<(Position, String)> = None;
        let settled = |latest: Option<(Position, String)>| match latest {
            Some((_, generation)) => Latest::At(generation),
            None => Latest::Absent,
        };
        for _ in 0..steps {
            if lo >= end {
                return Ok(settled(latest));
            }
            let found: Option<(String, Option<i64>)> = step
                .query_row(params![stream, &lo, &end], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()
                .map_err(be)?;
            let Some((key, at)) = found else {
                return Ok(settled(latest));
            };
            // The next bound, in three preferences: past this key's whole generation when
            // the policy named one, past a FOREIGN subject's whole range when the key
            // belongs to another subject nested in this one's, and otherwise past just
            // this key. Each is strictly greater than `key`, so the walk always advances.
            let mut bound = successor(&key);
            if let Some((candidate, generation)) = guard.identity.split_of(&key) {
                // A validated split's subject STARTS the key, and this key already begins
                // with `subject` (it came out of `subject`'s own range) - so a candidate
                // longer than `subject` is a subject NESTED inside it, with no further
                // test needed.
                let candidate = &key[candidate];
                if candidate == subject {
                    if let Some(span) = generation_span(&key, &generation) {
                        bound = bound.max(prefix_upper_bound(&key[..span]));
                    }
                    if let Some(at) = at {
                        let at = at as Position;
                        if latest.as_ref().is_none_or(|(seen, _)| at > *seen) {
                            latest = Some((at, key[generation].to_string()));
                        }
                    }
                } else if candidate.len() > subject.len() {
                    bound = bound.max(prefix_upper_bound(candidate));
                }
            }
            lo = bound;
        }
        // Out of steps: the subject's latest generation is UNDETERMINED, and an
        // undetermined probe never suppresses.
        Ok(Latest::Undetermined)
    }

    /// The metadata this store RECORDS for `event` - the caller's own, verbatim, except
    /// when the guard was not judging while this append ran.
    ///
    /// A guard that has stopped defending has to leave a trace, and the log is the only
    /// durable place it has: the degradation is invisible from the outside (an append
    /// that suppresses nothing looks exactly like an append with nothing to suppress),
    /// and the symptom shows up as a log growing again, days later, in another process.
    /// So the reason rides as ONE extra metadata pair - no new event type, no new
    /// serialized form, nothing to backfill - on the events this append was already
    /// writing, and any read of the store surfaces it.
    ///
    /// It is stamped ONLY on events of a type the policy COVERS. A domain event is never
    /// rewritten by a store that merely happened to be unhealthy while it landed: the
    /// derived-index events this rides on are the guard's own subject matter, and their
    /// metadata is the natural place for a fact about how they were admitted.
    fn recorded_meta(&self, event: &Event, degraded: Option<&'static str>) -> String {
        let Some(reason) = degraded else {
            return meta_json(&event.meta);
        };
        let covered = self
            .guard
            .as_ref()
            .is_some_and(|g| g.identity.covers(&event.type_));
        if !covered {
            return meta_json(&event.meta);
        }
        let mut meta = event.meta.clone();
        meta.insert(META_GUARD_DEGRADED.to_string(), reason.to_string());
        meta_json(&meta)
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
}

fn be<E: std::fmt::Display>(e: E) -> Error {
    Error::Backend(e.to_string())
}

/// A single-quoted SQL string literal for `s` (doubling any embedded quote). Used only
/// for values that are rendered into the guard's SQL once, at configuration time -
/// never for per-append data, which is always bound.
fn sql_literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// The SQL expression that reads an event's content key out of its metadata. The
/// content-key index is built on THIS expression and every probe selects on it, so they
/// are rendered from one function and the query planner sees identical text - a drift
/// there would be silent (still correct, but a whole-table walk per append, which is
/// the very cost this guard exists to avoid).
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
/// by any `json_extract` path, so every probe answers "not recorded" and the guard
/// SUPPRESSES NOTHING for such a policy - it appends, which is the fail-safe direction,
/// and never mistakes one key for another. Pinned by
/// `a_metadata_key_that_no_json_path_can_address_suppresses_nothing`.
fn key_expr(meta_key: &str) -> String {
    let path = format!(
        "$.\"{}\"",
        meta_key.replace('\\', "\\\\").replace('"', "\\\"")
    );
    format!("json_extract(meta, {})", sql_literal(&path))
}

/// The `IN (...)` list of the covered event types, rendered once at configuration.
///
/// A policy that covers NO type renders `NULL`, not an empty list: `type IN ()` is not
/// parsable SQL, while `type IN (NULL)` is never true - so a guard configured with no
/// covered types suppresses nothing, which is what covering no type means.
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

/// The definition SQLite has COMMITTED for the index named `name`, or `None` when no such
/// index exists. `sqlite_master` is the authority on what the database actually holds:
/// a statement that ran is not an index, and a transaction that rolled back leaves none.
fn committed_index_ddl(conn: &Connection, name: &str) -> Option<String> {
    conn.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?1",
        params![name],
        |r| r.get::<_, Option<String>>(0),
    )
    .optional()
    .ok()
    .flatten()
    .flatten()
}

/// Every committed content-key index that is NOT `keep` - the artifacts a policy that is
/// no longer configured left behind.
///
/// They are dead weight in the exact sense that matters to a store whose purpose is to
/// stay bounded: an index built for another metadata key answers no question this store
/// asks (its indexed expression reads a key nothing carries any more, so every entry in
/// it is NULL), yet SQLite still maintains it on every single insert and still stores it
/// in the file. Nobody else can identify them - only the handle holding the live policy
/// knows which name is the live one - so the build sweeps them.
///
/// The match is an EXACT prefix comparison on the shared stem, never a `LIKE` pattern:
/// the stem carries no wildcard today, and a comparison that would change meaning if it
/// ever did is not one to leave in a DROP path.
fn stale_content_key_indexes(conn: &Connection, keep: &str) -> Vec<String> {
    let mut names = Vec::new();
    let Ok(mut stmt) = conn.prepare(
        "SELECT name FROM sqlite_master \
         WHERE type = 'index' AND substr(name, 1, length(?1)) = ?1 AND name <> ?2 \
         ORDER BY name",
    ) else {
        return names;
    };
    let Ok(rows) = stmt.query_map(params![CONTENT_KEY_INDEX_STEM, keep], |r| {
        r.get::<_, String>(0)
    }) else {
        return names;
    };
    for row in rows.flatten() {
        names.push(row);
    }
    names
}

/// The smallest string strictly greater than `s`. Used as an INCLUSIVE lower bound that
/// excludes `s` itself and nothing else: no string sorts between `s` and `s` with a NUL
/// appended.
fn successor(s: &str) -> String {
    format!("{s}\u{0}")
}

/// The length of `key`'s leading span whose every continuation belongs to the SAME
/// generation as `key` - the prefix the walk skips to step past one whole generation in a
/// single index seek. `None` when no such span can be established, and the walk then
/// advances by one key.
///
/// It is `generation`'s end in `key` PLUS THE CHARACTER THAT FOLLOWS IT, and that extra
/// character is what makes the skip sound rather than merely fast. A generation may be a
/// string PREFIX of another one - a subject recorded at `h1` and later at `h12` - and a
/// skip that stopped at the generation's own end would jump past `<subject>h1`, which
/// `<subject>h12#0` sorts inside. The later generation would then be invisible, the walk
/// would report the superseded `h1` as current, and re-offering `h1` (a REVERT) would be
/// suppressed and lost. Including the delimiter narrows the skipped range to keys that
/// carry the generation AND the character that ends it, which `h12` does not.
///
/// So the span exists only while the generation is followed by something. A key whose
/// generation runs to its very END is not skippable at all - nothing there distinguishes
/// it from a longer generation that starts the same way - and it answers `None`, which
/// costs a step per key for a policy shaped that way and is correct for every policy.
///
/// `generation` is the RANGE the policy located the generation at, already validated by
/// [`ContentIdentity::split_of`] as lying inside `key` on character boundaries - so the
/// generation's end is read off the TYPE rather than recovered by comparing addresses,
/// and a policy cannot hand back something that merely looks like a slice of the key.
/// The store still parses no key format of its own; it only asks where the parts are.
fn generation_span(key: &str, generation: &Range<usize>) -> Option<usize> {
    let end = generation.end;
    let delimiter = key.get(end..)?.chars().next()?;
    Some(end + delimiter.len_utf8())
}

/// FNV-1a over the bytes of `s`. Used only to give an index NAME a per-policy suffix, so
/// two metadata keys that sanitize to the same identifier still get distinct artifacts.
fn fnv1a64(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// The exclusive upper bound of the half-open range that holds exactly the strings
/// beginning with `prefix`, under SQLite's default (byte-wise) TEXT collation: the
/// prefix with its last character bumped by one. This is what turns "every key naming
/// this subject" into an index RANGE SEEK rather than a scan-and-filter.
///
/// A prefix ending at the last representable character (or an empty one) has no such
/// successor; the bound then falls back to the prefix followed by the highest character
/// there is, which still bounds every practical key and never excludes one that a
/// simple `starts_with` would include for any prefix this crate mints.
fn prefix_upper_bound(prefix: &str) -> String {
    if let Some(last) = prefix.chars().next_back() {
        if let Some(next) = char::from_u32(last as u32 + 1) {
            let head = &prefix[..prefix.len() - last.len_utf8()];
            return format!("{head}{next}");
        }
    }
    format!("{prefix}{}", char::MAX)
}

fn to_nanos(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

fn from_nanos(n: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_nanos(n.max(0) as u64)
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
        // The content-key index the guard's probes require is settled BEFORE the append's
        // write transaction opens, and only when this batch has something suppressible in
        // it. Building it is a large write on an established log; inside the append's own
        // `BEGIN IMMEDIATE` its whole duration would be added to a window every other
        // process queues behind. `indexed` is false when the store has no guard, when the
        // batch has nothing the guard could suppress, or when the index is not there -
        // and a false answer suppresses nothing.
        let readiness = match &self.guard {
            Some(g) if Self::has_suppressible(g, events) => {
                if self.ensure_content_key_index(&mut guard, g) {
                    Readiness::Indexed
                } else {
                    Readiness::Unindexed
                }
            }
            _ => Readiness::Idle,
        };
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

        let count: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM events WHERE stream = ?1",
                [stream],
                |r| r.get(0),
            )
            .map_err(be)?;
        let last_revision: Revision = count - 1; // NO_STREAM (-1) when the stream is empty
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

        // The store stamps recorded_at on ingest (one clock per batch).
        let recorded_at = to_nanos(SystemTime::now());
        // One slot per handed event, in input order. A suppressed event takes no
        // per-stream revision, so the revision cursor advances only on a write and the
        // stream ends up advanced by exactly the events written.
        let mut placements: Vec<Option<Position>> = Vec::with_capacity(events.len());
        let mut revision = count;
        // Every suppression verdict is taken against the log as it stood when this
        // append began, BEFORE any of this batch's own rows exist.
        let verdicts = self.redundant_flags(&tx, stream, events, readiness)?;
        for (e, redundant) in events.iter().zip(verdicts.redundant) {
            if redundant {
                placements.push(None);
                continue;
            }
            tx.execute(
                "INSERT INTO events (stream, type, id, data, meta, valid_from, recorded_at, revision)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    stream,
                    e.type_,
                    e.id,
                    e.data,
                    self.recorded_meta(e, verdicts.degraded),
                    to_nanos(e.valid_from),
                    recorded_at,
                    revision
                ],
            )
            .map_err(be)?;
            revision += 1;
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

/// The storage-level content-identity guard, driven at the STORE PORT directly.
///
/// It is proved here rather than through an ingest run on purpose: the guard is
/// SUBORDINATE defense in depth under the ingest sink's own project-scoped dedup, and a
/// correct sink is built never to hand the store a redundant append in the first place -
/// so a proof that only ran through the sink would prove nothing about the guard, and a
/// defense whose only evidence is that nothing calls it is not evidence.
///
/// Lane-agnostic on purpose: the guard is type and string comparison plus two index
/// seeks, with no dependency on any extraction pass, so it keeps real coverage in both
/// feature lanes.
#[cfg(test)]
mod content_identity_guard {
    use super::*;
    use crate::contextgraph::{
        TYPE_CODE_ENTITY_EXTRACTED, TYPE_EDGE_INFERRED, TYPE_REVIEW_FINDING,
    };
    use crate::ingest::{DERIVED_INDEX_TYPES, META_REPLAY_KEY};

    /// Split a `<prefix>/<file>@<hash>#<i>` content key into `(the prefix every key
    /// naming the same file begins with, the content generation)`. This is the policy
    /// the composition root injects; it lives here in the test because the store must
    /// NEVER parse a key itself - the split is configuration, so that the format stays
    /// owned by the module that builds it. Splitting from the RIGHT is load-bearing: a
    /// real path may itself contain `@` or `#`.
    fn subject_of(key: &str) -> Option<(Range<usize>, Range<usize>)> {
        let (prefix, remainder) = key.split_once('/')?;
        if prefix.is_empty() || remainder.is_empty() {
            return None;
        }
        let (head, index) = key.rsplit_once('#')?;
        if index.is_empty() || !index.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let (file, hash) = head.rsplit_once('@')?;
        if file.len() <= prefix.len() + 1 || hash.is_empty() {
            return None;
        }
        let subject_end = file.len() + 1; // through the `@` that ends the subject
        Some((0..subject_end, subject_end..subject_end + hash.len()))
    }

    /// `subject_of` as the two slices, for assertions that read better as text.
    fn split(key: &str) -> Option<(&str, &str)> {
        let (subject, generation) = subject_of(key)?;
        Some((&key[subject], &key[generation]))
    }

    /// The policy under test: the real metadata key and the real derived-index type set
    /// the project's ingest layer uses, so the guard is exercised against the vocabulary
    /// it will actually be configured with rather than a fixture of its own.
    pub(super) fn identity() -> ContentIdentity {
        ContentIdentity::new(META_REPLAY_KEY, DERIVED_INDEX_TYPES, subject_of)
    }

    pub(super) fn keyed(type_: &str, key: &str) -> Event {
        Event::new(type_, b"payload".to_vec()).with_meta(META_REPLAY_KEY, key)
    }

    /// One file's batch at one content generation: the shape the ingest walk emits.
    fn batch(file: &str, hash: &str) -> Vec<Event> {
        vec![
            keyed(TYPE_CODE_ENTITY_EXTRACTED, &format!("gc/{file}@{hash}#0")),
            keyed(TYPE_EDGE_INFERRED, &format!("gc/{file}@{hash}#1")),
        ]
    }

    fn rows(store: &Store, stream: &str) -> usize {
        store
            .read_stream(stream, 0, Direction::Forward)
            .unwrap()
            .len()
    }

    #[test]
    fn subject_of_splits_from_the_right_so_a_path_may_carry_an_at_or_a_hash() {
        assert_eq!(
            split("gc/src/a.rs@h1#0"),
            Some(("gc/src/a.rs@", "h1")),
            "the subject prefix runs up to and including the generation separator"
        );
        assert_eq!(
            split("gd/a#1/b@2/c.md@deadbeef#12"),
            Some(("gd/a#1/b@2/c.md@", "deadbeef")),
            "a path containing both `@` and `#` still yields the whole path as the subject"
        );
        for malformed in ["gc/src/a.rs@h1", "gc/src/a.rs#0", "gc/@h1#0", "gc/a.rs@#0"] {
            assert_eq!(split(malformed), None, "{malformed:?} names no generation");
        }
    }

    /// THE CRITERION, all four clauses, over one log.
    #[test]
    fn a_still_current_generation_is_a_no_op_and_everything_else_still_appends() {
        let store = Store::open(":memory:")
            .unwrap()
            .with_content_identity(identity());

        // (1) A generation the log has never seen appends in full.
        let h1 = batch("src/a.rs", "h1");
        let first = store.append("run", ExpectedRevision::Any, &h1).unwrap();
        assert_eq!(
            first.written(),
            2,
            "a first-seen generation is written whole"
        );
        assert_eq!(rows(&store, "run"), 2);

        // (2) THE NO-OP: re-offering the SAME generation, which is still this file's
        // latest recorded one, writes nothing - and says so per event, with no position
        // anywhere in the report.
        let again = store.append("run", ExpectedRevision::Any, &h1).unwrap();
        assert_eq!(
            again.placements(),
            &[None, None],
            "every event of an already-recorded current generation is a storage no-op"
        );
        assert_eq!(
            again.handed(),
            2,
            "the report still names every handed event"
        );
        assert_eq!(
            again.last(),
            None,
            "an append that wrote nothing reports an absence, never a fabricated position 0"
        );
        assert_eq!(rows(&store, "run"), 2, "the log did not grow");

        // (3) A NEW generation of the same file appends: the file changed.
        let h2 = batch("src/a.rs", "h2");
        assert_eq!(
            store
                .append("run", ExpectedRevision::Any, &h2)
                .unwrap()
                .written(),
            2,
            "a changed file re-emits its whole batch"
        );
        assert_eq!(rows(&store, "run"), 4);

        // (4) THE REVERT: h1's keys are recorded, but the file has MOVED PAST them, so
        // they are not redundant and MUST append. An ever-recorded test would swallow
        // this and strand the projection on h2 forever, with no recovery - re-folding
        // the log would replay the same suppression.
        let reverted = store.append("run", ExpectedRevision::Any, &h1).unwrap();
        assert_eq!(
            reverted.written(),
            2,
            "a generation the file has moved past is a CHANGE and must append"
        );
        assert_eq!(rows(&store, "run"), 6);
        // ...and once it has, h1 is current again, so offering it again is a no-op and
        // h2 - now the superseded one - appends.
        assert_eq!(
            store
                .append("run", ExpectedRevision::Any, &h1)
                .unwrap()
                .written(),
            0,
            "the reverted generation is the current one now, so re-offering it is a no-op"
        );
        assert_eq!(
            store
                .append("run", ExpectedRevision::Any, &h2)
                .unwrap()
                .written(),
            2,
            "the generation the file moved away from appends again"
        );

        // (5) A DOMAIN EVENT never gets content identity, even when its payload is
        // identical and even when its replay key is spelled exactly like a content key:
        // two identical review findings mean the finding was raised twice.
        let finding = keyed(TYPE_REVIEW_FINDING, "gc/src/a.rs@h1#0");
        let before = rows(&store, "run");
        for _ in 0..2 {
            assert_eq!(
                store
                    .append("run", ExpectedRevision::Any, std::slice::from_ref(&finding))
                    .unwrap()
                    .written(),
                1,
                "a domain event appends per append, whatever its metadata says"
            );
        }
        assert_eq!(rows(&store, "run"), before + 2);
    }

    /// A SHORT WRITE, reported honestly: a batch mixing an already-current derived
    /// event with a genuinely new one writes one row and names WHICH one it wrote.
    /// This is the case the arithmetic the shared fold authority used to do cannot
    /// survive - it would stamp the suppressed event at a position the store never
    /// issued - and it is why the report is per event rather than a single "last".
    #[test]
    fn a_partially_suppressed_append_reports_which_events_it_wrote() {
        let store = Store::open(":memory:")
            .unwrap()
            .with_content_identity(identity());
        let h1 = batch("src/a.rs", "h1");
        store.append("run", ExpectedRevision::Any, &h1).unwrap();

        let mixed = vec![
            h1[0].clone(),
            keyed(TYPE_REVIEW_FINDING, "not-a-content-key"),
        ];
        let appended = store.append("run", ExpectedRevision::Any, &mixed).unwrap();

        assert_eq!(appended.handed(), 2);
        assert_eq!(
            appended.written(),
            1,
            "exactly one of the two events was written"
        );
        assert_eq!(
            appended.placements()[0],
            None,
            "the already-current derived event is suppressed"
        );
        let placed: Vec<(usize, Position)> = appended.placed().collect();
        assert_eq!(placed.len(), 1);
        assert_eq!(
            placed[0].0, 1,
            "the report names WHICH input event was written"
        );

        // The position reported is the one the log actually holds that event at, and
        // the stream advanced by exactly one revision - a suppressed event consumes no
        // revision, so the stream stays contiguous.
        let stream = store.read_stream("run", 0, Direction::Forward).unwrap();
        let held = stream
            .iter()
            .find(|e| e.position == placed[0].1)
            .expect("the store holds an event at the reported position");
        assert_eq!(held.id, mixed[1].id, "and it is the event the report names");
        assert_eq!(
            stream.iter().map(|e| e.revision).collect::<Vec<_>>(),
            (0..stream.len() as Revision).collect::<Vec<_>>(),
            "per-stream revisions stay contiguous: a suppressed event consumes none"
        );
    }

    /// The recorded state the guard tests is THE LOG'S, not one connection's or one
    /// process's. A SECOND handle on the same on-disk file - the shape of a fresh
    /// `rigger step` or a cold `rigger graph build`, each its own process - must reach
    /// the same verdict on its FIRST append. An in-memory seen-set would answer "never
    /// seen" here and defend nothing, because the duplication this exists to stop is
    /// cross-process.
    #[test]
    fn a_second_handle_on_the_same_log_suppresses_on_its_first_append() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let path = path.to_str().unwrap();

        let writer = Store::open(path).unwrap().with_content_identity(identity());
        let h1 = batch("src/a.rs", "h1");
        writer.append("run", ExpectedRevision::Any, &h1).unwrap();
        drop(writer);

        let fresh = Store::open(path).unwrap().with_content_identity(identity());
        let appended = fresh.append("run", ExpectedRevision::Any, &h1).unwrap();
        assert_eq!(
            appended.written(),
            0,
            "a fresh process's FIRST append gets the same answer a long-lived one's would"
        );
        assert_eq!(rows(&fresh, "run"), 2, "and the log did not grow");
    }

    /// NOT INERT ON A LOG THAT ALREADY EXISTS. The guard asks its question of the
    /// content keys the log ALREADY carries - no new event type, no new metadata, no
    /// backfill - so a log written by a build that had no guard at all is defended from
    /// the very first append against it.
    #[test]
    fn a_log_written_with_no_guard_at_all_is_defended_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let path = path.to_str().unwrap();

        // Written by a store with NO content identity configured: exactly the bytes an
        // earlier binary would have left.
        let legacy = Store::open(path).unwrap();
        let h1 = batch("src/a.rs", "h1");
        legacy.append("run", ExpectedRevision::Any, &h1).unwrap();
        legacy.append("run", ExpectedRevision::Any, &h1).unwrap();
        assert_eq!(rows(&legacy, "run"), 4, "no guard means no suppression");
        drop(legacy);

        let guarded = Store::open(path).unwrap().with_content_identity(identity());
        assert_eq!(
            guarded
                .append("run", ExpectedRevision::Any, &h1)
                .unwrap()
                .written(),
            0,
            "the guard reads keys the log already carries, so it defends a pre-existing log"
        );
        assert_eq!(rows(&guarded, "run"), 4);
    }

    /// An unconfigured store has no guard and appends everything through. That is the
    /// FAIL-SAFE direction: a store with no policy can only ever write MORE, never drop.
    #[test]
    fn an_unconfigured_store_appends_everything_through() {
        let store = Store::open(":memory:").unwrap();
        let h1 = batch("src/a.rs", "h1");
        for _ in 0..3 {
            assert_eq!(
                store
                    .append("run", ExpectedRevision::Any, &h1)
                    .unwrap()
                    .written(),
                2
            );
        }
        assert_eq!(rows(&store, "run"), 6);
    }

    /// Two files whose paths differ only after a shared prefix must never share a
    /// subject: the range the guard seeks is bounded by the subject's own separator, so
    /// `src/a.rs` can never be read as a generation of `src/a.rs.bak`.
    #[test]
    fn one_files_generations_never_leak_into_another_files_subject() {
        let store = Store::open(":memory:")
            .unwrap()
            .with_content_identity(identity());
        store
            .append("run", ExpectedRevision::Any, &batch("src/a.rs", "h1"))
            .unwrap();
        store
            .append("run", ExpectedRevision::Any, &batch("src/a.rs.bak", "h9"))
            .unwrap();
        // The sibling's later batch must not make `src/a.rs@h1` look superseded.
        assert_eq!(
            store
                .append("run", ExpectedRevision::Any, &batch("src/a.rs", "h1"))
                .unwrap()
                .written(),
            0,
            "another file's newer batch is not this file's generation"
        );
        // ...and the sibling is still guarded on its own account.
        assert_eq!(
            store
                .append("run", ExpectedRevision::Any, &batch("src/a.rs.bak", "h9"))
                .unwrap()
                .written(),
            0
        );
        assert_eq!(rows(&store, "run"), 4);
    }

    /// Two projects sharing one backend mint the IDENTICAL content key for a shared
    /// relative path with identical content - the namespacing decorator gives each its
    /// own stream, and that is the boundary the guard must respect. A probe that read
    /// the whole log would drop the second project's genuinely-new fact.
    #[test]
    fn one_projects_recorded_key_never_suppresses_anothers() {
        let store = Store::open(":memory:")
            .unwrap()
            .with_content_identity(identity());
        let h1 = batch("src/a.rs", "h1");

        assert_eq!(
            store
                .append("proj-a-run", ExpectedRevision::Any, &h1)
                .unwrap()
                .written(),
            2
        );
        assert_eq!(
            store
                .append("proj-b-run", ExpectedRevision::Any, &h1)
                .unwrap()
                .written(),
            2,
            "another project's identical key is another project's fact and must append"
        );
        // ...and each project is still guarded on its own stream.
        assert_eq!(
            store
                .append("proj-b-run", ExpectedRevision::Any, &h1)
                .unwrap()
                .written(),
            0
        );
        assert_eq!(rows(&store, "proj-a-run"), 2);
        assert_eq!(rows(&store, "proj-b-run"), 2);
    }

    /// THE PRECONDITION IS THE WHOLE KEY, NOT THE GENERATION. A batch that GREW while its
    /// generation stayed the same - the same file re-recorded with an event its earlier
    /// recording did not carry - must append the events the log has never seen, and only
    /// those.
    ///
    /// This is what the exact-key `recorded` probe buys, and nothing else buys it: the
    /// generation test alone answers "this file is already at h1" for the new event too,
    /// and suppresses it. It would then be absent from the log forever, because every
    /// later run asks the same question and gets the same answer - a permanent hole in
    /// the index with no re-derivation that can fill it.
    #[test]
    fn a_batch_that_grew_at_the_same_generation_appends_exactly_its_unrecorded_events() {
        let store = Store::open(":memory:")
            .unwrap()
            .with_content_identity(identity());
        let first = [keyed(TYPE_CODE_ENTITY_EXTRACTED, "gc/src/a.rs@h1#0")];
        assert_eq!(
            store
                .append("run", ExpectedRevision::Any, &first)
                .unwrap()
                .placements(),
            [Some(1)]
        );
        // The SAME generation, now carrying a second event.
        let grown = [
            keyed(TYPE_CODE_ENTITY_EXTRACTED, "gc/src/a.rs@h1#0"),
            keyed(TYPE_EDGE_INFERRED, "gc/src/a.rs@h1#1"),
        ];
        assert_eq!(
            store
                .append("run", ExpectedRevision::Any, &grown)
                .unwrap()
                .placements(),
            [None, Some(2)],
            "the recorded key is a no-op and the UNRECORDED one is written - a generation \
             test alone would swallow it"
        );
        assert_eq!(rows(&store, "run"), 2);
    }

    /// The type gate lives in the probes as well as ahead of them, and this pins the one
    /// inside the walk: a NON-DERIVED event is not eligible to DATE a generation, however
    /// its replay key is spelled.
    ///
    /// The population is real - a run's own lifecycle events, gate verdicts and review
    /// findings all carry a `replay_key`, and nothing stops one from being shaped like a
    /// content key - so a walk that counted them would read whichever generation a domain
    /// event happened to name LAST as the file's current one, and then suppress the
    /// genuinely current batch.
    #[test]
    fn a_non_derived_event_never_dates_a_generation_however_its_key_is_spelled() {
        let store = Store::open(":memory:")
            .unwrap()
            .with_content_identity(identity());
        store
            .append("run", ExpectedRevision::Any, &batch("src/a.rs", "h1"))
            .unwrap();
        store
            .append("run", ExpectedRevision::Any, &batch("src/a.rs", "h2"))
            .unwrap();
        // Recorded LAST, and spelled exactly like h1's leading content key.
        store
            .append(
                "run",
                ExpectedRevision::Any,
                &[keyed(TYPE_REVIEW_FINDING, "gc/src/a.rs@h1#0")],
            )
            .unwrap();

        // h2 is still the file's latest DERIVED generation, so re-offering it is a no-op.
        // A walk that let the review finding date h1 would call h1 current, read h2 as
        // superseded, and append it again.
        let again = batch("src/a.rs", "h2");
        assert_eq!(
            store
                .append("run", ExpectedRevision::Any, &again)
                .unwrap()
                .placements(),
            [None, None]
        );
        assert_eq!(rows(&store, "run"), 5, "the log did not grow");
    }

    /// And the type gate inside the RECORDED probe: a non-derived event carrying a
    /// content-shaped key never makes a derived event look already recorded.
    ///
    /// Without it the domain event answers the precondition for a content key the log has
    /// never held on a derived event, and - the file's generation being current - the
    /// derived event is suppressed and lost.
    #[test]
    fn a_non_derived_events_key_never_makes_a_derived_event_look_recorded() {
        let store = Store::open(":memory:")
            .unwrap()
            .with_content_identity(identity());
        // h1 IS this file's current generation...
        store
            .append(
                "run",
                ExpectedRevision::Any,
                &[keyed(TYPE_CODE_ENTITY_EXTRACTED, "gc/src/a.rs@h1#1")],
            )
            .unwrap();
        // ...and a DOMAIN event happens to carry h1's other key.
        store
            .append(
                "run",
                ExpectedRevision::Any,
                &[keyed(TYPE_REVIEW_FINDING, "gc/src/a.rs@h1#0")],
            )
            .unwrap();
        assert_eq!(
            store
                .append(
                    "run",
                    ExpectedRevision::Any,
                    &[keyed(TYPE_CODE_ENTITY_EXTRACTED, "gc/src/a.rs@h1#0")],
                )
                .unwrap()
                .placements(),
            [Some(3)],
            "no derived event has ever carried this key, so it is not recorded"
        );
    }

    /// TWO POLICIES, TWO ARTIFACTS. The index's definition carries the configured
    /// metadata key (that is the expression it indexes), so its NAME carries it too: a
    /// store configured with a different key must not inherit - and then silently fall
    /// off - the first policy's index.
    #[test]
    fn a_second_policy_gets_its_own_index_rather_than_inheriting_the_firsts() {
        assert_ne!(
            Store::content_key_index_name("replay_key"),
            Store::content_key_index_name("content_key"),
            "an index built on one metadata key answers no question about another"
        );
        let name = Store::content_key_index_name("replay_key");
        assert!(
            Store::content_key_index_ddl(&name, "replay_key").contains("replay_key"),
            "and the definition is what makes that so"
        );
    }

    /// A COMMITTED DEFINITION IS THE GATE, NEVER A NAME. An artifact sitting under the
    /// right name with the wrong definition is exactly the state a name check waves
    /// through, and every probe then falls off the index onto a walk of the stream.
    #[test]
    fn an_index_with_the_right_name_and_the_wrong_definition_is_rebuilt() {
        let store = Store::open(":memory:")
            .unwrap()
            .with_content_identity(identity());
        let (name, wanted) = {
            let g = store.guard.as_ref().expect("configured");
            (g.index_name.clone(), g.index_ddl.clone())
        };
        // An index of the right name that indexes the wrong thing entirely.
        store
            .conn
            .lock()
            .unwrap()
            .execute_batch(&format!("CREATE INDEX {name} ON events(stream)"))
            .unwrap();

        store
            .append("run", ExpectedRevision::Any, &batch("src/a.rs", "h1"))
            .unwrap();

        assert_eq!(
            committed_index_ddl(&store.conn.lock().unwrap(), &name).as_deref(),
            Some(wanted.as_str()),
            "the stale artifact must be replaced, not accepted"
        );
    }

    /// A BUILD THAT NEVER COMMITTED IS NOT REMEMBERED AS DONE, and until it does commit
    /// the guard SUPPRESSES NOTHING.
    ///
    /// Both halves matter and neither is optional. A handle that latched "attempted" on
    /// the way in would believe forever, and silently, that it has an index it does not
    /// have - and would then pay an unbounded walk of the stream on every probe. And with
    /// no index the fail-safe direction is to APPEND: a duplicate row is recoverable, and
    /// an append that costs a table walk under the write lock is what takes a run down.
    #[test]
    fn an_index_that_cannot_be_built_suppresses_nothing_and_is_retried_not_remembered() {
        let store = Store::open(":memory:")
            .unwrap()
            .with_content_identity(identity());
        let name = store.guard.as_ref().expect("configured").index_name.clone();
        // Occupy the index's name with a TABLE, so the build can never commit.
        store
            .conn
            .lock()
            .unwrap()
            .execute_batch(&format!("CREATE TABLE {name}(blocker)"))
            .unwrap();

        let h1 = batch("src/a.rs", "h1");
        store.append("run", ExpectedRevision::Any, &h1).unwrap();
        assert_eq!(
            store
                .append("run", ExpectedRevision::Any, &h1)
                .unwrap()
                .written(),
            2,
            "with no index the guard suppresses nothing - the redundant batch appends"
        );
        assert_eq!(rows(&store, "run"), 4);
        assert!(
            !store
                .guard
                .as_ref()
                .expect("configured")
                .index_ready
                .load(Ordering::SeqCst),
            "a build that never committed must never be remembered as done"
        );

        // Free the name: the very next append re-attempts, and the guard comes back.
        store
            .conn
            .lock()
            .unwrap()
            .execute_batch(&format!("DROP TABLE {name}"))
            .unwrap();
        assert_eq!(
            store
                .append("run", ExpectedRevision::Any, &h1)
                .unwrap()
                .written(),
            0,
            "the retry builds the index and the guard resumes"
        );
        assert_eq!(rows(&store, "run"), 4, "and the log did not grow");
    }

    /// WHAT THIS PINS: that every probe is answered by SEEKING the content-key index ON
    /// ITS KEY TERM - not that any particular wall-clock cost holds. A query plan states
    /// the access path and nothing about duration, and the two must not be confused: the
    /// walk above is bounded by the number of generations it steps through, and THAT is
    /// what bounds the probe's cost. The plan is what stops the step itself from
    /// silently becoming a table walk.
    ///
    /// The key term is the whole point of the assertion. The probes qualify for the
    /// EXPRESSION index only while their SQL mirrors the indexed expression exactly, and
    /// a drift there is silent - the planner still uses the index, but only for the
    /// `stream` equality, and then walks that stream's every event per probe. A plan
    /// that names the index and constrains nothing but the stream is exactly the
    /// degradation this test exists to catch, so naming the index is not enough: the
    /// plan must show the key expression constrained too.
    #[test]
    fn every_probe_seeks_the_content_key_index_on_its_key_term() {
        let store = Store::open(":memory:")
            .unwrap()
            .with_content_identity(identity());
        // Drive one suppressible append so the lazily created index exists.
        let h1 = batch("src/a.rs", "h1");
        store.append("run", ExpectedRevision::Any, &h1).unwrap();
        store.append("run", ExpectedRevision::Any, &h1).unwrap();

        let guard = store.guard.as_ref().expect("configured");
        let index = guard.index_name.as_str();
        let conn = store.conn.lock().unwrap();
        let probes: [(&String, Vec<&dyn rusqlite::ToSql>); 2] = [
            (&guard.recorded_sql, vec![&"gc/src/a.rs@h1#0", &"run"]),
            (
                &guard.step_sql,
                vec![&"run", &"gc/src/a.rs@", &"gc/src/a.rsA"],
            ),
        ];
        for (sql, args) in probes {
            let plan: Vec<String> = conn
                .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
                .unwrap()
                .query_map(rusqlite::params_from_iter(args), |r| r.get::<_, String>(3))
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
            let plan = plan.join(" | ");
            assert!(
                plan.contains(index),
                "the probe must seek {index}; plan was {plan}\nsql: {sql}"
            );
            // SQLite renders an indexed expression's constraint as `<expr>` in a plan, so
            // `(stream=? AND <expr>...)` is the seek this guard needs and `(stream=?)`
            // alone is the walk it must never do.
            assert!(
                plan.contains("stream=? AND <expr>"),
                "the probe must constrain the CONTENT KEY on the index, not just the \
                 stream; plan was {plan}\nsql: {sql}"
            );
            assert!(
                !plan.contains("SCAN events"),
                "the probe must never scan the events table; plan was {plan}\nsql: {sql}"
            );
            assert!(
                !plan.contains("TEMP B-TREE"),
                "the probe must never sort - a sort is the whole range materialised; \
                 plan was {plan}\nsql: {sql}"
            );
        }
    }

    /// A GENERATION THAT IS A STRING PREFIX OF ANOTHER is still found. `h1` and `h12` are
    /// two unrelated generations, but `<subject>h12#0` sorts INSIDE the key range that
    /// begins `<subject>h1`, so a walk that stepped past "everything beginning with this
    /// key's subject and generation" would skip `h12` outright, read the file as still at
    /// `h1`, and then suppress a re-offer of `h1` - which by then is a REVERT, and the
    /// only thing that could put the graph back on the file's earlier content.
    ///
    /// This is the case that decides how far the generation skip may reach: past the
    /// character that DELIMITS the generation, never merely past the generation.
    #[test]
    fn a_generation_that_is_a_string_prefix_of_a_later_one_is_still_found() {
        let store = Store::open(":memory:")
            .unwrap()
            .with_content_identity(identity());
        store
            .append("run", ExpectedRevision::Any, &batch("src/a.rs", "h1"))
            .unwrap();
        store
            .append("run", ExpectedRevision::Any, &batch("src/a.rs", "h12"))
            .unwrap();

        // The file is at h12, so h1 is SUPERSEDED and re-offering it must append.
        assert_eq!(
            store
                .append("run", ExpectedRevision::Any, &batch("src/a.rs", "h1"))
                .unwrap()
                .written(),
            2,
            "h12 is this file's latest generation, so h1 is a revert and must append"
        );
        // ...and now h1 is current again, so h12 in turn appends and h1 does not.
        assert_eq!(
            store
                .append("run", ExpectedRevision::Any, &batch("src/a.rs", "h1"))
                .unwrap()
                .written(),
            0
        );
        assert_eq!(
            store
                .append("run", ExpectedRevision::Any, &batch("src/a.rs", "h12"))
                .unwrap()
                .written(),
            2
        );
    }

    /// THE BOUND, driven rather than asserted about: the walk costs one step per recorded
    /// GENERATION of the subject (plus the one step that proves the range is exhausted),
    /// and NOT one per event - the sixty sibling keys three batches mint here cost three
    /// range jumps between them, not sixty.
    ///
    /// It is proved by starving it. Handed one step fewer than the subject has
    /// generations, the walk must answer UNDETERMINED - `None` - and never the best
    /// generation it happened to reach before running out, because "the latest I got to"
    /// would suppress a revert. Handed exactly enough, it answers. A walk that visited
    /// events rather than generations would need sixty-one steps and would fail the
    /// second assertion; a walk that returned its partial best would fail the first.
    #[test]
    fn the_walk_spends_one_step_per_generation_and_an_exhausted_budget_answers_nothing() {
        let store = Store::open(":memory:")
            .unwrap()
            .with_content_identity(identity());
        // Three generations of ONE file, each a wide batch: 3 generations, 60 events.
        let wide = |hash: &str| -> Vec<Event> {
            (0..20)
                .map(|i| {
                    keyed(
                        TYPE_CODE_ENTITY_EXTRACTED,
                        &format!("gc/src/a.rs@{hash}#{i}"),
                    )
                })
                .collect()
        };
        for hash in ["h1", "h2", "h3"] {
            store
                .append("run", ExpectedRevision::Any, &wide(hash))
                .unwrap();
        }
        assert_eq!(rows(&store, "run"), 60);

        let guard = store.guard.as_ref().expect("configured");
        let mut conn = store.conn.lock().unwrap();
        let tx = conn.transaction().unwrap();
        let within = |steps| {
            store
                .latest_generation_within(&tx, guard, "run", "gc/src/a.rs@", steps)
                .unwrap()
        };
        assert_eq!(
            within(4),
            Latest::At("h3".to_string()),
            "three generations plus the step that proves the range exhausted is all it costs"
        );
        assert_eq!(
            within(3),
            Latest::Undetermined,
            "a budget that cannot cover every generation must answer UNDETERMINED, so the \
             append goes through - and it must say UNDETERMINED rather than ABSENT, which \
             is what makes the degradation recordable"
        );
        assert_eq!(
            store
                .latest_generation_within(&tx, guard, "run", "gc/src/never.rs@", 4)
                .unwrap(),
            Latest::Absent,
            "a subject the log has never recorded is an ANSWER, not a degradation"
        );
    }

    /// THE RENDERED SQL IS SQL, WHATEVER THE POLICY IS SPELLED WITH. The metadata key and
    /// the covered type names are configuration (a consumer's strings, not this module's)
    /// rendered into statement text once, so they are the one place a quote could end a
    /// literal early and leave the remainder as stray SQL. Both go through the same
    /// escaper, and the store is driven here with a policy carrying an apostrophe (the
    /// character that ends a SQL literal) and a backslash (the one that escapes inside a
    /// JSON path) in BOTH the metadata key and a covered type name.
    #[test]
    fn a_policy_spelled_with_quotes_and_backslashes_still_guards() {
        let awkward = "re'play\\key";
        let awkward_type = "Ty'pe\\A";
        let store = Store::open(":memory:")
            .unwrap()
            .with_content_identity(ContentIdentity::new(awkward, [awkward_type], subject_of));
        let h1 = [Event::new(awkward_type, b"payload".to_vec()).with_meta(awkward, "gc/a.rs@h1#0")];
        // A broken rendering shows up as a backend error (unparsable) or as a probe that
        // never matches (mis-escaped); the two appends below separate both from working.
        assert_eq!(
            store
                .append("run", ExpectedRevision::Any, &h1)
                .expect("the rendered probes must be valid SQL")
                .written(),
            1
        );
        assert_eq!(
            store
                .append("run", ExpectedRevision::Any, &h1)
                .expect("and stay valid on the suppressing path")
                .written(),
            0,
            "the guard reads the awkward metadata key just as well as a plain one"
        );
    }

    /// A metadata key NO JSON path can address fails SAFE. SQLite's path parser accepts an
    /// escaped backslash inside a quoted member name but not an escaped double quote, so a
    /// key carrying a `"` is unreachable by `json_extract` however it is spelled. The guard
    /// must then suppress NOTHING - append, the fail-safe direction - and must never
    /// silently read some OTHER key's value as this one's.
    #[test]
    fn a_metadata_key_that_no_json_path_can_address_suppresses_nothing() {
        let unreachable = "quo\"ted";
        let store = Store::open(":memory:")
            .unwrap()
            .with_content_identity(ContentIdentity::new(
                unreachable,
                DERIVED_INDEX_TYPES,
                subject_of,
            ));
        let h1 = [Event::new(TYPE_CODE_ENTITY_EXTRACTED, b"payload".to_vec())
            .with_meta(unreachable, "gc/a.rs@h1#0")];
        store.append("run", ExpectedRevision::Any, &h1).unwrap();
        assert_eq!(
            store
                .append("run", ExpectedRevision::Any, &h1)
                .expect("an unaddressable key is not an error")
                .written(),
            1,
            "a probe that cannot see the key answers 'not recorded', so the append goes \
             through"
        );
        assert_eq!(rows(&store, "run"), 2);
    }

    /// A policy that covers NO type suppresses nothing - and, just as importantly, renders
    /// SQL that PARSES. `type IN ()` is not valid SQL, so a store configured this way would
    /// fail every append rather than simply guard nothing; `type IN (NULL)` is never true,
    /// which is exactly what covering no type means.
    #[test]
    fn a_policy_that_covers_no_type_renders_parsable_sql_and_suppresses_nothing() {
        assert_eq!(type_list(&[]), "NULL");
        let store = Store::open(":memory:")
            .unwrap()
            .with_content_identity(ContentIdentity::new(
                META_REPLAY_KEY,
                Vec::<String>::new(),
                subject_of,
            ));
        let h1 = batch("src/a.rs", "h1");
        store.append("run", ExpectedRevision::Any, &h1).unwrap();
        assert_eq!(
            store
                .append("run", ExpectedRevision::Any, &h1)
                .expect("an empty type set must still render SQL that parses")
                .written(),
            2,
            "covering no type suppresses nothing"
        );
    }

    /// The subject range is a half-open interval, and its upper bound is the prefix with
    /// its last character bumped - which is what makes "every key naming this subject" an
    /// index range seek instead of a scan-and-filter.
    #[test]
    fn the_subject_range_holds_exactly_the_keys_that_begin_with_the_subject() {
        let lo = "gc/src/a.rs@";
        let hi = prefix_upper_bound(lo);
        assert_eq!(
            hi, "gc/src/a.rsA",
            "the bound is the prefix with its last char bumped"
        );
        let in_range = |k: &str| k >= lo && k < hi.as_str();
        assert!(
            in_range("gc/src/a.rs@h1#0"),
            "the subject's own keys are inside"
        );
        assert!(in_range("gc/src/a.rs@h2#7"), "every generation of it too");
        // A sibling path that merely SHARES a leading run of characters is outside the
        // range in both directions, so no file's batch can retire another's.
        assert!(!in_range("gc/src/a.rs.bak@h1#0"));
        assert!(!in_range("gc/src/a.rsZ@h1#0"));
        assert!(!in_range("gc/src/b.rs@h1#0"));
        assert_eq!(prefix_upper_bound(""), char::MAX.to_string());
    }

    /// A store's other guarded policy, keyed on a DIFFERENT metadata key - so it needs a
    /// different index and answers a different question.
    fn other_identity() -> ContentIdentity {
        ContentIdentity::new(OTHER_META_KEY, DERIVED_INDEX_TYPES, subject_of)
    }

    const OTHER_META_KEY: &str = "other_replay_key";

    fn other_keyed(type_: &str, key: &str) -> Event {
        Event::new(type_, b"payload".to_vec()).with_meta(OTHER_META_KEY, key)
    }

    /// The readiness latch is a fact about A POLICY'S INDEX, so it belongs to the policy
    /// and dies with it. RECONFIGURING MUST BUILD THE NEW POLICY'S OWN INDEX.
    ///
    /// A latch held beside the policy instead of on it is a second value that can
    /// disagree with the first, and this is the disagreement: the handle has latched
    /// "ready" about the index the FIRST policy built, the second policy needs a
    /// different index entirely (its expression reads a different metadata key), and the
    /// latch answers for it. The guard then believes every probe is a seek while every
    /// probe is a full table walk with a `json_extract` per row - inside the append's
    /// exclusive write transaction, which every other process on that store is queued
    /// behind. Nothing about the ANSWERS changes, which is why only the artifact can
    /// witness it.
    #[test]
    fn reconfiguring_builds_the_new_policys_index_and_never_inherits_the_olds_readiness() {
        let store = Store::open(":memory:")
            .unwrap()
            .with_content_identity(identity());
        store
            .append("run", ExpectedRevision::Any, &batch("src/a.rs", "h1"))
            .unwrap();
        assert!(
            store
                .guard
                .as_ref()
                .expect("configured")
                .index_ready
                .load(Ordering::SeqCst),
            "the first policy's index is built and latched"
        );

        let store = store.with_content_identity(other_identity());
        let (name, wanted) = {
            let g = store.guard.as_ref().expect("reconfigured");
            assert!(
                !g.index_ready.load(Ordering::SeqCst),
                "a new policy starts unlatched: the latch is created with it, not \
                 inherited from the policy it replaces"
            );
            (g.index_name.clone(), g.index_ddl.clone())
        };

        store
            .append(
                "run",
                ExpectedRevision::Any,
                &[other_keyed(TYPE_CODE_ENTITY_EXTRACTED, "gc/src/b.rs@h1#0")],
            )
            .unwrap();

        assert_eq!(
            committed_index_ddl(&store.conn.lock().unwrap(), &name).as_deref(),
            Some(wanted.as_str()),
            "the reconfigured store builds ITS OWN index; a latch that outlived the \
             policy it described would leave every probe walking the table"
        );
    }

    /// AN INDEX ANOTHER HANDLE ALREADY COMMITTED IS NOT REBUILT. This is the success
    /// path of a cold store with more than one appender, not a misuse path.
    ///
    /// Both handles here are cold, and the decision to build is necessarily taken
    /// OUTSIDE the write lock - so on a cold store every handle that starts before the
    /// first one commits arrives at the build having seen no index. The build's first
    /// act under the lock is therefore to look again. Without that re-read this racer
    /// drops and recreates the artifact the winner just committed, holding the exclusive
    /// write lock for the whole rebuild, and every bystander process appending an
    /// ordinary event in that window fails on the busy timeout rather than waiting.
    ///
    /// `PRAGMA schema_version` is the witness because it counts SCHEMA CHANGES and
    /// nothing else: a build that writes nothing leaves it exactly where it was, while a
    /// needless `DROP` plus `CREATE` moves it by two.
    #[test]
    fn a_racing_cold_handle_finds_the_index_under_the_lock_and_rebuilds_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run.db");
        let path = path.to_str().unwrap().to_string();
        let schema_version = |at: &str| -> i64 {
            Connection::open(at)
                .unwrap()
                .query_row("PRAGMA schema_version", [], |r| r.get(0))
                .unwrap()
        };

        let winner = Store::open(&path)
            .unwrap()
            .with_content_identity(identity());
        // The racer is opened and configured while the store is still COLD, which is
        // exactly the state a handle is in when it passes the check taken outside the
        // lock and decides to build.
        let racer = Store::open(&path)
            .unwrap()
            .with_content_identity(identity());
        let name = racer.guard.as_ref().expect("configured").index_name.clone();
        assert!(
            committed_index_ddl(&racer.conn.lock().unwrap(), &name).is_none(),
            "both handles start with nothing committed to find"
        );

        winner
            .append("run", ExpectedRevision::Any, &batch("src/a.rs", "h1"))
            .unwrap();
        let built = schema_version(&path);

        {
            let guard = racer.guard.as_ref().expect("configured");
            let mut conn = racer.conn.lock().unwrap();
            racer.build_content_key_index(&mut conn, guard);
        }

        assert_eq!(
            schema_version(&path),
            built,
            "the racer writes no DDL at all: one build, not one per handle"
        );
        assert_eq!(
            racer
                .append("run", ExpectedRevision::Any, &batch("src/a.rs", "h1"))
                .unwrap()
                .written(),
            0,
            "and it guards with the artifact it did not rebuild"
        );
    }

    /// AN ARTIFACT NO CONFIGURED POLICY USES IS RECLAIMED, by the one handle that can
    /// know it is dead.
    ///
    /// An index built for another metadata key answers no question this store asks - its
    /// indexed expression reads a key nothing carries any more, so every entry in it is
    /// NULL - yet SQLite still maintains it on every insert and still keeps it in the
    /// file. On a spec whose whole purpose is to BOUND the store, leaving one behind per
    /// policy change is the guard growing the thing it was built to shrink.
    #[test]
    fn an_index_left_by_a_policy_that_is_no_longer_configured_is_dropped() {
        let store = Store::open(":memory:")
            .unwrap()
            .with_content_identity(identity());
        store
            .append("run", ExpectedRevision::Any, &batch("src/a.rs", "h1"))
            .unwrap();
        let abandoned = store.guard.as_ref().expect("configured").index_name.clone();
        assert!(
            committed_index_ddl(&store.conn.lock().unwrap(), &abandoned).is_some(),
            "the first policy's artifact is committed"
        );

        let store = store.with_content_identity(other_identity());
        let live = store
            .guard
            .as_ref()
            .expect("reconfigured")
            .index_name
            .clone();
        store
            .append(
                "run",
                ExpectedRevision::Any,
                &[other_keyed(TYPE_CODE_ENTITY_EXTRACTED, "gc/src/b.rs@h1#0")],
            )
            .unwrap();

        let conn = store.conn.lock().unwrap();
        assert!(
            committed_index_ddl(&conn, &abandoned).is_none(),
            "the dead artifact is reclaimed, not left to be maintained on every insert"
        );
        assert!(
            committed_index_ddl(&conn, &live).is_some(),
            "and the live one is there"
        );
        assert!(
            stale_content_key_indexes(&conn, &live).is_empty(),
            "nothing content-keyed is left over"
        );
    }

    /// THE TYPE GATE IN RUST, DRIVEN BY THE ONE SHAPE THAT REACHES IT.
    ///
    /// Every other test of the type rule is answered before this gate: a solo domain
    /// event never makes the batch suppressible, and a domain event carrying a key of no
    /// content-key shape is stopped by the split instead. The gate itself is only
    /// reached by a domain event that is (a) in a batch the guard IS judging and (b)
    /// carrying a well-formed key that is genuinely its subject's CURRENT generation -
    /// which is to say, everything a suppressible event has except the type. Delete the
    /// three lines and this event is dropped; nothing else in the suite notices.
    #[test]
    fn a_domain_event_survives_a_judged_batch_even_carrying_a_live_content_key() {
        let store = Store::open(":memory:")
            .unwrap()
            .with_content_identity(identity());
        store
            .append("run", ExpectedRevision::Any, &batch("src/a.rs", "h1"))
            .unwrap();

        // The derived event is redundant, so the guard is judging; the finding carries
        // the OTHER key of that same still-current generation, already recorded.
        let mixed = vec![
            keyed(TYPE_CODE_ENTITY_EXTRACTED, "gc/src/a.rs@h1#0"),
            keyed(TYPE_REVIEW_FINDING, "gc/src/a.rs@h1#1"),
        ];
        let appended = store.append("run", ExpectedRevision::Any, &mixed).unwrap();

        assert_eq!(
            appended.written(),
            1,
            "the derived duplicate is suppressed and the domain event is not"
        );
        assert!(
            appended.placements()[0].is_none() && appended.placements()[1].is_some(),
            "and the report says WHICH: {:?}",
            appended.placements()
        );
        assert_eq!(
            rows(&store, "run"),
            3,
            "the finding is a row in the log, on a key the guard would have suppressed \
             for any covered type"
        );
    }

    /// A GUARD THAT HAS STOPPED DEFENDING SAYS SO, IN THE LOG.
    ///
    /// Both off states are invisible from outside: an append that suppresses nothing
    /// looks exactly like an append with nothing to suppress, and the symptom - a log
    /// growing without bound again - surfaces days later in another process. So the
    /// reason is recorded where every other fact here is recorded, as one metadata pair
    /// on the covered events the append was already writing. No new event type, nothing
    /// to backfill, and any read of the store surfaces it.
    #[test]
    fn an_append_written_without_an_index_records_that_the_guard_was_not_judging() {
        let store = Store::open(":memory:")
            .unwrap()
            .with_content_identity(identity());
        let name = store.guard.as_ref().expect("configured").index_name.clone();
        // Occupy the index's name with a TABLE, so the build can never commit.
        store
            .conn
            .lock()
            .unwrap()
            .execute_batch(&format!("CREATE TABLE {name}(blocker)"))
            .unwrap();

        let mut mixed = batch("src/a.rs", "h1");
        mixed.push(keyed(TYPE_REVIEW_FINDING, "gc/src/a.rs@h1#9"));
        store.append("run", ExpectedRevision::Any, &mixed).unwrap();

        let recorded = store.read_stream("run", 0, Direction::Forward).unwrap();
        assert_eq!(recorded.len(), 3);
        for event in recorded.iter().take(2) {
            assert_eq!(
                event.meta.get(META_GUARD_DEGRADED).map(String::as_str),
                Some(GUARD_DEGRADED_NO_INDEX),
                "every covered event written by a guard that could not judge says so"
            );
        }
        assert_eq!(
            recorded[2].meta.get(META_GUARD_DEGRADED),
            None,
            "a domain event is never rewritten by a store that merely happened to be \
             unhealthy while it landed"
        );

        // Free the name: the guard comes back, and a healthy append stamps nothing.
        store
            .conn
            .lock()
            .unwrap()
            .execute_batch(&format!("DROP TABLE {name}"))
            .unwrap();
        store
            .append("run", ExpectedRevision::Any, &batch("src/b.rs", "h1"))
            .unwrap();
        let healthy = store.read_stream("run", 0, Direction::Forward).unwrap();
        assert!(
            healthy[3..]
                .iter()
                .all(|e| !e.meta.contains_key(META_GUARD_DEGRADED)),
            "a guard that is judging stamps nothing: the mark means what it says"
        );
    }

    /// The OTHER off state, and it is a different fact: the index is there and the walk
    /// still could not answer, because this one subject has recorded more generations
    /// than the probe's step budget allows it to step through. Nothing is suppressed
    /// (the fail-safe direction), and the events written say WHY - `generations-exceeded`
    /// rather than `no-index`, because the two ask for different remedies.
    #[test]
    fn an_append_judged_by_an_exhausted_walk_records_which_defence_gave_way() {
        let store = Store::open(":memory:")
            .unwrap()
            .with_content_identity(identity());
        // One subject, more generations than the walk may step through.
        let generations = LATEST_GENERATION_STEPS + 6;
        for i in 0..generations {
            store
                .append(
                    "run",
                    ExpectedRevision::Any,
                    &[keyed(
                        TYPE_CODE_ENTITY_EXTRACTED,
                        &format!("gc/src/a.rs@h{i:05}#0"),
                    )],
                )
                .unwrap();
        }

        // Re-offering a RECORDED key is what sends the probe walking.
        let again = vec![keyed(TYPE_CODE_ENTITY_EXTRACTED, "gc/src/a.rs@h00000#0")];
        let appended = store.append("run", ExpectedRevision::Any, &again).unwrap();

        assert_eq!(
            appended.written(),
            1,
            "an undetermined probe never suppresses - it appends"
        );
        let last = store
            .read_stream("run", 0, Direction::Forward)
            .unwrap()
            .pop()
            .expect("the append landed");
        assert_eq!(
            last.meta.get(META_GUARD_DEGRADED).map(String::as_str),
            Some(GUARD_DEGRADED_UNDETERMINED),
            "the row carries WHICH defence gave way, so the duplicate it let through is \
             explainable instead of mysterious"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_the_contract() {
        crate::eventstore::contract::assert_contract(&Store::open(":memory:").unwrap());
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

    /// The GUARDED twin of the test above, and the shape a real run has: several
    /// PROCESSES, each its own connection and its own handle, offering the SAME file's
    /// batch to one on-disk log at once.
    ///
    /// Three things have to hold together here and none of them can be seen from a
    /// single-handle test. The index every probe needs is built lazily, so all eight
    /// handles reach for it at once and it must be built ONCE, by whichever gets the
    /// write lock first, with the rest reading the committed definition rather than
    /// racing a second build. The verdict must be the LOG'S, so the seven handles that
    /// arrive after the first commit must see the batch recorded even though their own
    /// process never wrote it. And every one of these appends holds `BEGIN IMMEDIATE`,
    /// so a probe or a build that overran the busy timeout would surface right here as a
    /// hard `database is locked` - the failure mode that costs a run a self-report, not a
    /// row.
    #[test]
    fn concurrent_guarded_handles_build_one_index_and_record_one_copy() {
        use super::content_identity_guard::{identity, keyed};
        use crate::contextgraph::TYPE_CODE_ENTITY_EXTRACTED;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run.db");
        let path = path.to_str().unwrap().to_string();

        const HANDLES: usize = 8;
        // One file's batch, wide enough that a probe per event is actually exercised.
        let batch: Vec<Event> = (0..24)
            .map(|i| {
                keyed(
                    TYPE_CODE_ENTITY_EXTRACTED,
                    &format!("gc/src/wide.rs@h1#{i}"),
                )
            })
            .collect();

        // Open every handle up front (serialized) so only the appends race.
        let stores: Vec<Arc<Store>> = (0..HANDLES)
            .map(|_| {
                Arc::new(
                    Store::open(&path)
                        .unwrap()
                        .with_content_identity(identity()),
                )
            })
            .collect();
        let barrier = Arc::new(std::sync::Barrier::new(HANDLES));
        // `PRAGMA schema_version` counts SCHEMA CHANGES and nothing else, so it is the
        // exact witness for "how many times was this index built": one `CREATE INDEX` is
        // one step, and a needless rebuild is a `DROP` plus a `CREATE`, which is two.
        let schema_version = || -> i64 {
            Connection::open(&path)
                .unwrap()
                .query_row("PRAGMA schema_version", [], |r| r.get(0))
                .unwrap()
        };
        let before = schema_version();

        let handles: Vec<_> = stores
            .into_iter()
            .map(|store| {
                let bar = Arc::clone(&barrier);
                let batch = batch.clone();
                std::thread::spawn(move || {
                    bar.wait();
                    match store.append("run", ExpectedRevision::Any, &batch) {
                        Ok(appended) => (appended.written(), 0usize),
                        Err(Error::Conflict { .. }) => (0, 0),
                        Err(_) => (0, 1),
                    }
                })
            })
            .collect();
        let (written, hard_errs) = handles.into_iter().map(|h| h.join().unwrap()).fold(
            (Vec::new(), 0usize),
            |(mut w, e), (wrote, err)| {
                w.push(wrote);
                (w, e + err)
            },
        );

        assert_eq!(
            hard_errs, 0,
            "a guarded append must queue like any other - a lock error here is a lost \
             self-report, not a lost row"
        );
        let winners: Vec<usize> = written.iter().copied().filter(|w| *w > 0).collect();
        assert_eq!(
            winners,
            vec![batch.len()],
            "exactly ONE handle writes the batch and every other suppresses it whole; \
             written per handle was {written:?}"
        );
        assert_eq!(
            schema_version() - before,
            1,
            "and the index is built ONCE for all {HANDLES} handles. Every one of them is \
             cold and decides to build outside the write lock, so without a re-read once \
             the lock is held each would drop and recreate what the last one committed - \
             a multi-hundred-megabyte artifact rebuilt per handle, serially, with every \
             bystander appender queued behind it on the busy timeout"
        );

        let reader = Store::open(&path).unwrap();
        assert_eq!(
            reader
                .read_stream("run", 0, Direction::Forward)
                .unwrap()
                .len(),
            batch.len(),
            "the log holds exactly one copy of the file's batch"
        );
    }
}
