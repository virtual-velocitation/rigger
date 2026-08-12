//! SQLite-backed EventStore. A single connection behind a mutex serializes
//! writes, so concurrent appenders queue instead of deadlocking on the
//! lock-upgrade (SQLITE_BUSY) class. Per-stream revisions and a `UNIQUE(stream,
//! revision)` index give optimistic concurrency; `$all` is `ORDER BY position`.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use super::{
    Appended, ContentIdentity, Direction, Error, Event, EventStore, ExpectedRevision, Filter,
    Position, Revision, Subscription, NO_STREAM,
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
    /// Whether the content-key index this handle's policy needs is COMMITTED, with the
    /// definition this policy renders. Latched true only after that definition has been
    /// read back out of `sqlite_master` in its own statement, so a build that was rolled
    /// back can never leave the handle believing the index is there: an unlatched handle
    /// re-checks (and re-attempts) on the next append. While it is false the guard
    /// suppresses NOTHING - see [`Store::redundant_flags`].
    index_ready: AtomicBool,
}

/// The configured guard: the policy, the index its probes require, and the probe
/// statements - all rendered from the policy ONCE, at configuration time, so the text
/// the query planner sees can never drift from the indexed expression (they are built
/// from the same key expression).
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
}

impl Store {
    /// Open (creating if needed) the store at path. Use ":memory:" in tests.
    pub fn open(path: &str) -> Result<Self, Error> {
        let conn = Connection::open(path).map_err(be)?;
        conn.execute_batch(SCHEMA).map_err(be)?;
        Ok(Store {
            conn: Arc::new(Mutex::new(conn)),
            guard: None,
            index_ready: AtomicBool::new(false),
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
    /// Spelled WITHOUT `IF NOT EXISTS` on purpose: SQLite strips that clause when it
    /// stores a definition in `sqlite_master`, so omitting it makes this text exactly
    /// what a committed index reads back as - which is what lets
    /// [`Store::ensure_content_key_index`] compare the COMMITTED DEFINITION rather than
    /// trust a name.
    ///
    /// `(stream, <content key>, position)` in that order, because that is the shape the
    /// probes ask for: the stream is an equality (the project boundary), the content key
    /// is an equality for two probes and a half-open RANGE for the third, and the
    /// position rides along so a key's last recording is read off the index itself.
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
    ///
    /// It is reached only from an append that carries a suppressible event, so a store
    /// that is only ever read, or only ever written with uncovered types, never builds it.
    fn ensure_content_key_index(&self, conn: &mut Connection, guard: &Guard) -> bool {
        if self.index_ready.load(Ordering::SeqCst) {
            return true;
        }
        if committed_index_ddl(conn, &guard.index_name).as_deref() == Some(guard.index_ddl.as_str())
        {
            self.index_ready.store(true, Ordering::SeqCst);
            return true;
        }
        let built = (|| -> rusqlite::Result<()> {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute_batch(&format!(
                "DROP INDEX IF EXISTS {};\n{};",
                guard.index_name, guard.index_ddl
            ))?;
            tx.commit()
        })();
        // The verdict is the COMMITTED state, not the outcome of the statement: a batch
        // that reported success but was rolled back at commit must not latch readiness.
        let _ = built;
        let ready = committed_index_ddl(conn, &guard.index_name).as_deref()
            == Some(guard.index_ddl.as_str());
        if ready {
            self.index_ready.store(true, Ordering::SeqCst);
        }
        ready
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
        indexed: bool,
    ) -> Result<Vec<bool>, Error> {
        let Some(guard) = &self.guard else {
            return Ok(vec![false; events.len()]);
        };
        // NO USABLE INDEX, NO SUPPRESSION. Without it every probe becomes a walk of the
        // table, and a guard that costs an unbounded walk per append is worse than the
        // duplication it removes - so the store appends, which is the fail-safe direction
        // this whole layer is built on (it can only ever write MORE, never drop).
        if !indexed {
            return Ok(vec![false; events.len()]);
        }
        // One latest-generation lookup per DISTINCT subject in the batch, not per event:
        // a file's whole batch shares one subject.
        let mut current: std::collections::HashMap<String, Option<String>> =
            std::collections::HashMap::new();
        let mut flags = Vec::with_capacity(events.len());
        for event in events {
            flags.push(self.is_redundant(tx, guard, stream, event, &mut current)?);
        }
        Ok(flags)
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
        current: &mut std::collections::HashMap<String, Option<String>>,
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
        Ok(current
            .get(subject)
            .and_then(|g| g.as_deref())
            .is_some_and(|latest| latest == generation))
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
    /// Stepping past a generation uses the byte offsets of the slices the POLICY handed
    /// back, never a separator this module knows: `subject_of` returns borrows INTO the
    /// key, so the key's leading `subject + generation` span is arithmetic on those
    /// borrows. The store still parses no key format of its own.
    ///
    /// Candidates are filtered by asking the policy for each one's subject, because a
    /// prefix range is a superset: a file whose path itself contains the generation
    /// separator (`vendor/pkg@1.2.3/a.rs` beside a file named `vendor/pkg`) sits inside
    /// the shorter subject's range while belonging to a different subject entirely, and
    /// letting it answer would let one file's generation retire another's. A foreign
    /// subject is skipped WHOLE for the same reason a generation is: by its own range.
    ///
    /// Answers `None` for a subject with no recorded generation AND for a walk that ran
    /// out of steps; both mean "not established as current", and neither suppresses.
    fn latest_generation(
        &self,
        tx: &rusqlite::Transaction<'_>,
        guard: &Guard,
        stream: &str,
        subject: &str,
    ) -> Result<Option<String>, Error> {
        self.latest_generation_within(tx, guard, stream, subject, LATEST_GENERATION_STEPS)
    }

    /// [`Store::latest_generation`] with the step budget given explicitly, so the bound
    /// itself is drivable: a walk handed fewer steps than the subject has generations
    /// must answer `None` (undetermined), never the best generation it happened to reach.
    fn latest_generation_within(
        &self,
        tx: &rusqlite::Transaction<'_>,
        guard: &Guard,
        stream: &str,
        subject: &str,
        steps: usize,
    ) -> Result<Option<String>, Error> {
        let end = prefix_upper_bound(subject);
        let mut step = tx.prepare_cached(&guard.step_sql).map_err(be)?;
        let mut lo = subject.to_string();
        let mut latest: Option<(Position, String)> = None;
        for _ in 0..steps {
            if lo >= end {
                return Ok(latest.map(|(_, generation)| generation));
            }
            let found: Option<(String, Option<i64>)> = step
                .query_row(params![stream, &lo, &end], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()
                .map_err(be)?;
            let Some((key, at)) = found else {
                return Ok(latest.map(|(_, generation)| generation));
            };
            // The next bound, in three preferences: past this key's whole generation when
            // the policy named one, past a FOREIGN subject's whole range when the key
            // belongs to another subject nested in this one's, and otherwise past just
            // this key. Each is strictly greater than `key`, so the walk always advances.
            let mut bound = successor(&key);
            if let Some((candidate, generation)) = guard.identity.subject_of(&key) {
                if candidate == subject {
                    if let Some(span) = leading_span(&key, generation) {
                        bound = bound.max(prefix_upper_bound(&key[..span]));
                    }
                    if let Some(at) = at {
                        let at = at as Position;
                        if latest.as_ref().is_none_or(|(seen, _)| at > *seen) {
                            latest = Some((at, generation.to_string()));
                        }
                    }
                } else if candidate.len() > subject.len() && key.starts_with(candidate) {
                    bound = bound.max(prefix_upper_bound(candidate));
                }
            }
            lo = bound;
        }
        // Out of steps: the subject's latest generation is UNDETERMINED, and an
        // undetermined probe never suppresses.
        Ok(None)
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
/// Two escapes, both required and neither optional: the JSON path spells a quote or a
/// backslash inside its quoted member name with a backslash, and the finished path is
/// then a SQL string literal like any other, so it goes through [`sql_literal`] rather
/// than being pasted between hand-written quotes.
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

/// The smallest string strictly greater than `s`. Used as an INCLUSIVE lower bound that
/// excludes `s` itself and nothing else: no string sorts between `s` and `s` with a NUL
/// appended.
fn successor(s: &str) -> String {
    format!("{s}\u{0}")
}

/// The length of `whole`'s leading span that ends where `part` ends, when `part` is a
/// borrow INTO `whole`; `None` when it is not (a policy that returned a slice of some
/// other string). Pure offset arithmetic on two slices of one allocation - it is how the
/// walk steps past a whole generation without knowing any key's format.
fn leading_span(whole: &str, part: &str) -> Option<usize> {
    let base = whole.as_ptr() as usize;
    let at = part.as_ptr() as usize;
    let end = at.checked_sub(base)?.checked_add(part.len())?;
    (end <= whole.len() && whole.is_char_boundary(end)).then_some(end)
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
        let indexed = match &self.guard {
            Some(g) if Self::has_suppressible(g, events) => {
                self.ensure_content_key_index(&mut guard, g)
            }
            _ => false,
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
        let redundant = self.redundant_flags(&tx, stream, events, indexed)?;
        for (e, redundant) in events.iter().zip(redundant) {
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
                    meta_json(&e.meta),
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
    fn subject_of(key: &str) -> Option<(&str, &str)> {
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
        Some((&key[..file.len() + 1], hash))
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
            subject_of("gc/src/a.rs@h1#0"),
            Some(("gc/src/a.rs@", "h1")),
            "the subject prefix runs up to and including the generation separator"
        );
        assert_eq!(
            subject_of("gd/a#1/b@2/c.md@deadbeef#12"),
            Some(("gd/a#1/b@2/c.md@", "deadbeef")),
            "a path containing both `@` and `#` still yields the whole path as the subject"
        );
        for malformed in ["gc/src/a.rs@h1", "gc/src/a.rs#0", "gc/@h1#0", "gc/a.rs@#0"] {
            assert_eq!(
                subject_of(malformed),
                None,
                "{malformed:?} names no generation"
            );
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
            !store.index_ready.load(Ordering::SeqCst),
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
            within(4).as_deref(),
            Some("h3"),
            "three generations plus the step that proves the range exhausted is all it costs"
        );
        assert_eq!(
            within(3),
            None,
            "a budget that cannot cover every generation must answer UNDETERMINED, so the \
             append goes through"
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
