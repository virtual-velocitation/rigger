//! The append-only, bi-temporal event store: an immutable log of facts the ledger
//! and context graph are projected from. `EventStore` is the port; `sqlite` is the
//! default adapter. The trait mirrors KurrentDB's primitives - per-stream append
//! with optimistic concurrency, a global $all order, per-stream revisions, and
//! catch-up subscriptions - so a backend swaps without changing the rest of Rigger.

pub mod namespace;
pub mod sqlite;

// The KurrentDB adapter is always compiled in (spec 47): the shared-store backend is
// a first-class product capability reachable in the default build via a runtime flag,
// never a recompile - so the module is not gated behind a cargo feature.
pub mod kurrentdb;

#[cfg(test)]
pub mod contract;

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};

use thiserror::Error;

/// Position is an event's place in the global $all order: store-assigned and only
/// ever increasing. It is a single opaque ordering value: callers compare and
/// checkpoint it, never decompose it.
///
/// SQLite assigns it directly as the 1-based row position. KurrentDB's native
/// `$all` position is a `(commit, prepare)` pair; the adapter exposes the
/// **commit position** as this `Position` and reconstructs the pair as
/// `(commit, commit)` when resuming a read or subscription. That round-trips
/// faithfully because KurrentDB orders and seeks `$all` by commit position, and
/// every record Rigger writes is a single-event append whose own commit and
/// prepare positions coincide (the prepare half of a *start* position only
/// disambiguates records that share a commit, which Rigger never produces). A
/// position returned from `read_all`/`subscribe_all` therefore resolves back to
/// the same logical location when fed into the next resume.
pub type Position = u64;

/// Revision is an event's place within its own stream: 0-based, so the first event
/// in a stream is revision 0. An empty stream sits at [`NO_STREAM`].
pub type Revision = i64;

/// The revision of a stream that does not yet exist.
pub const NO_STREAM: Revision = -1;

/// Read direction over a stream or the global log.
#[derive(Clone, Copy, Debug)]
pub enum Direction {
    Forward,
    Backward,
}

/// The optimistic-concurrency expectation for [`EventStore::append`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpectedRevision {
    /// No concurrency check.
    Any,
    /// The stream must not yet exist (its last revision is [`NO_STREAM`]).
    NoStream,
    /// The stream's current last revision must equal this exactly.
    Exact(Revision),
}

/// A read/subscription filter over the global log.
#[derive(Clone, Debug, Default)]
pub struct Filter {
    pub stream_prefix: Option<String>,
}

/// Event is a single immutable fact. Callers populate the input fields; the store
/// stamps `recorded_at`, `position`, and `revision` on append (and `stream` to the
/// target stream). `valid_from` is the bi-temporal valid-time - when the fact
/// became true - and defaults to the append time unless the caller sets it.
#[derive(Clone, Debug)]
pub struct Event {
    pub id: String,
    pub stream: String,
    pub type_: String,
    pub data: Vec<u8>,
    /// Causation, correlation, and actor metadata.
    pub meta: BTreeMap<String, String>,
    /// When the fact became true (caller-supplied; defaults to the append time).
    pub valid_from: SystemTime,
    /// When the store ingested it (store-stamped).
    pub recorded_at: SystemTime,
    pub position: Position,
    pub revision: Revision,
}

impl Event {
    /// A new event with a fresh id. The store stamps `stream`, `recorded_at`,
    /// `position`, and `revision` on append; `valid_from` defaults to now and may
    /// be overridden with [`Event::with_valid_from`].
    pub fn new(type_: impl Into<String>, data: Vec<u8>) -> Self {
        let now = SystemTime::now();
        Event {
            id: uuid::Uuid::new_v4().to_string(),
            stream: String::new(),
            type_: type_.into(),
            data,
            meta: BTreeMap::new(),
            valid_from: now,
            recorded_at: now,
            position: 0,
            revision: NO_STREAM,
        }
    }

    /// Builder: set a metadata entry (causation / correlation / actor).
    pub fn with_meta(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.meta.insert(key.into(), value.into());
        self
    }

    /// Builder: set the valid-from time (when the fact became true).
    pub fn with_valid_from(mut self, t: SystemTime) -> Self {
        self.valid_from = t;
        self
    }
}

/// What an [`EventStore::append`] actually wrote: ONE entry per event the store was
/// handed, in the order it was handed them. `Some(position)` is the global
/// [`Position`] the store ISSUED for that event; `None` is an event the store
/// recognised as already recorded and did not write.
///
/// The report exists because an append may write FEWER events than it was handed (an
/// adapter may carry a content-identity guard - see [`ContentIdentity`]), and a caller
/// that folds what it appended has to stamp each event with the position the store
/// issued. Deriving positions arithmetically from a single "last" value is unsound in
/// two independent ways: it assumes every handed event was written, and it assumes a
/// batch lands at CONSECUTIVE positions, which this port has never promised (it
/// promises DISTINCT, strictly increasing positions - a backend whose positions are
/// byte offsets satisfies that and is not consecutive).
///
/// There is NO in-band sentinel anywhere on this path. An append that wrote nothing
/// reports it as an explicit absence ([`Appended::last`] is `None`), never as a
/// fabricated position `0`: the graph projection's applied ledger is keyed BY
/// position, so a fabricated `0` would permanently mark position 0 applied and swallow
/// the genuine event recorded there.
///
/// The type is a newtype over its per-event slots precisely so no caller can build an
/// inconsistent report: there is no separate "written" flag to disagree with the
/// positions, and the count of written events is derived, never stored.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Appended {
    placements: Vec<Option<Position>>,
}

impl Appended {
    /// The report for an append that wrote EVERY event it was handed, at `positions`
    /// (in input order). This is what every append that suppresses nothing returns.
    pub fn all(positions: Vec<Position>) -> Self {
        Appended {
            placements: positions.into_iter().map(Some).collect(),
        }
    }

    /// The report for an append that wrote only some of the events it was handed:
    /// one slot per handed event, in input order, `None` where the store suppressed.
    pub fn from_placements(placements: Vec<Option<Position>>) -> Self {
        Appended { placements }
    }

    /// The per-event slots, in input order - the shape a caller zips against the batch
    /// it handed the store.
    pub fn placements(&self) -> &[Option<Position>] {
        &self.placements
    }

    /// The events that were WRITTEN, as `(index into the handed batch, position)`.
    pub fn placed(&self) -> impl Iterator<Item = (usize, Position)> + '_ {
        self.placements
            .iter()
            .enumerate()
            .filter_map(|(i, p)| p.map(|p| (i, p)))
    }

    /// How many of the handed events were written.
    pub fn written(&self) -> usize {
        self.placements.iter().filter(|p| p.is_some()).count()
    }

    /// How many events the store was handed (written and suppressed alike).
    pub fn handed(&self) -> usize {
        self.placements.len()
    }

    /// The position of the LAST event written, or `None` when the append wrote
    /// nothing at all (an empty batch, or every event suppressed). Positions are
    /// strictly increasing, so this is also the greatest position written.
    ///
    /// This is the BATCH question, and its absence is a legitimate answer: a batch may
    /// hold nothing to write, or hold only events an adapter's guard recognised. A caller
    /// that handed over exactly ONE event is asking a different question and asks
    /// [`Appended::one`] instead.
    pub fn last(&self) -> Option<Position> {
        self.placements.iter().rev().find_map(|p| *p)
    }

    /// The ONE position issued for the ONE event a single-event append handed over, or
    /// the error a store that did not write it has earned. `what` names the thing being
    /// recorded, so the failure reads as what the caller asked for rather than as an
    /// internal type name.
    ///
    /// This is the single authority for what an absence MEANS to a caller that appended
    /// exactly one event: nothing was recorded, and the caller cannot say why. Such a
    /// caller has no second answer available - it cannot fold, cite, or print a position
    /// the store never issued - so every one of them reports the absence identically here,
    /// rather than each deciding for itself whether to fabricate a position, return a
    /// silent success, discard the report, or explain a cause it cannot know.
    ///
    /// A report that does not answer exactly one event fails too: handing back the last of
    /// several positions would answer a question the caller did not ask.
    pub fn one(&self, what: &str) -> Result<Position, Error> {
        // No "event store" prefix: `Error::Backend`'s own Display already opens with one,
        // and a message that repeats it reads as a stutter to the operator.
        match self.placements.as_slice() {
            [Some(p)] => Ok(*p),
            [] | [None] => Err(Error::Backend(format!(
                "reported writing nothing for {what}"
            ))),
            many => Err(Error::Backend(format!(
                "answered a single-event append for {what} with {} slots",
                many.len()
            ))),
        }
    }
}

/// The content-identity policy an adapter's append guard enforces: WHICH event types
/// carry content identity, WHERE an event carries its content key, and how that key
/// splits into the SUBJECT it describes and the content GENERATION it belongs to.
///
/// This is CONFIGURATION, injected at the composition root, never vocabulary the store
/// owns: the event types whose payload is a re-derivable index of the project's own
/// sources are knowledge of the layer that derives them, and the store is the lower
/// port. Handing the policy in keeps the store free of any dependency on that layer
/// and keeps the key format owned by the module that BUILDS it - the store never
/// parses a key itself, it asks [`ContentIdentity::subject_of`].
///
/// A store with no policy configured has no guard and appends everything through,
/// which is the fail-safe direction: an unconfigured store can only ever write MORE,
/// never drop.
///
/// The policy also carries the VALID-TIME PARTITION a compaction needs
/// ([`with_reasserting_types`](Self::with_reasserting_types)) - which of the covered
/// types re-assert a fact in place rather than superseding the subject's prior
/// recording. That belongs here, on the one injected value, and not as a second
/// positional list beside it: a compaction that deletes a key's earlier recordings has
/// to know whether the earliest recorded valid-time is the one the projection holds,
/// and a per-type rule expressed twice can be handed in the wrong order and can drift a
/// call site at a time. It is still injected knowledge, still just type names, so the
/// store learns nothing about the fold it could not be told.
#[derive(Clone)]
pub struct ContentIdentity {
    meta_key: String,
    types: Vec<String>,
    /// The covered types whose recordings RE-ASSERT, or `None` when this policy has
    /// never been told the partition. `None` is not "no type re-asserts": the two are
    /// different states on purpose, because a compaction cannot act correctly on the
    /// first and must say so rather than guess (see [`reasserts`](Self::reasserts)).
    reasserting: Option<Vec<String>>,
    split: ContentKeySplit,
}

/// How a content key splits, expressed as WHERE the two parts lie in the key rather
/// than as two strings: `(the byte range of the subject prefix, the byte range of the
/// content generation)`, both indexing the key handed in.
///
/// Ranges, not `&str`, because the obligation is otherwise unstatable. An adapter's
/// guard has to step past a whole generation in one index seek, which needs the
/// generation's OFFSET inside the key - and a `fn(&str) -> Option<(&str, &str)>` can
/// be satisfied by a policy that returns owned or `'static` slices that merely LOOK
/// right (`Some(("gc/a.rs@", "h1"))` compiles and coerces). The guard would then be
/// unable to locate the generation, would quietly stop suppressing, and would report
/// exactly what an unguarded store reports - a guard that has silently stopped
/// guarding, indistinguishable from a working one. A byte range cannot lie about
/// where it points, so the type carries the obligation the doc used to carry alone.
pub type ContentKeySplit = fn(&str) -> Option<(Range<usize>, Range<usize>)>;

impl ContentIdentity {
    /// Build the policy. `meta_key` is the metadata key an identified event carries
    /// its content key under; `types` are the event types that carry content identity
    /// (every other type keeps per-append identity untouched); `split` locates, WITHIN
    /// a content key, `(the prefix EVERY key naming the same subject begins with, the
    /// content generation this key belongs to)` and answers `None` for a key that is
    /// not of the caller's content-key shape.
    ///
    /// The ranges `split` returns are CHECKED, never trusted (see
    /// [`ContentIdentity::split_of`]): a policy whose ranges do not describe a
    /// well-formed split of the key it was handed is treated as naming no generation
    /// at all, which appends - the fail-safe direction.
    pub fn new(
        meta_key: impl Into<String>,
        types: impl IntoIterator<Item = impl Into<String>>,
        split: ContentKeySplit,
    ) -> Self {
        ContentIdentity {
            meta_key: meta_key.into(),
            types: types.into_iter().map(Into::into).collect(),
            reasserting: None,
            split,
        }
    }

    /// Declare the VALID-TIME PARTITION over this policy's covered types: `reasserting`
    /// names the types whose recordings RE-ASSERT a fact that was already true, so the
    /// EARLIEST recorded valid-time is the one the projection holds. Every covered type
    /// NOT named here SUPERSEDES: its latest recording's own valid-time is the one a
    /// fold arrives at.
    ///
    /// Declaring it is TOTAL - one call answers for every covered type - which is why a
    /// compaction may act on it and why a policy that has never had this called on it is
    /// a different state from one that declared an empty list. WHICH types are which is
    /// a fact about the projection, not about the store, so it arrives here as data from
    /// the layer that folds them; declaring it on the one policy value keeps it from
    /// being re-spelled at a call site.
    pub fn with_reasserting_types(
        mut self,
        reasserting: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.reasserting = Some(reasserting.into_iter().map(Into::into).collect());
        self
    }

    /// Whether `type_`'s recordings RE-ASSERT (`Some(true)`) or SUPERSEDE (`Some(false)`),
    /// or `None` when this policy was never told the partition at all.
    ///
    /// `None` is the answer a caller must handle rather than default, and it is why this
    /// returns an `Option` instead of a `bool`. Neither default is safe: guessing
    /// "supersedes" re-dates every re-asserted fact to whichever recording a compaction
    /// happened to keep, and guessing "re-asserts" drags a superseded fact back to a date
    /// its fold retired. Both move the live graph silently, so the only correct answer to
    /// an undeclared partition is to refuse to act on it.
    pub fn reasserts(&self, type_: &str) -> Option<bool> {
        let declared = self.reasserting.as_ref()?;
        Some(declared.iter().any(|t| t == type_))
    }

    /// The declared re-asserting types, or `None` when the partition was never declared -
    /// so a caller can check the declaration itself (every name in it must be a type this
    /// policy covers, or the declaration is about a policy other than this one).
    pub fn reasserting(&self) -> Option<&[String]> {
        self.reasserting.as_deref()
    }

    /// The metadata key an identified event carries its content key under.
    pub fn meta_key(&self) -> &str {
        &self.meta_key
    }

    /// The event types that carry content identity.
    pub fn types(&self) -> &[String] {
        &self.types
    }

    /// This policy's key SPLIT, so a caller that needs the same key form under a different
    /// metadata key or a different covered-type list builds a variant of THIS policy rather
    /// than inventing a second parser of the same key shape. Read alongside
    /// [`meta_key`](Self::meta_key) and [`types`](Self::types); the checked reading of it is
    /// [`split_of`](Self::split_of), which is what every store uses.
    pub fn split(&self) -> ContentKeySplit {
        self.split
    }

    /// Whether `type_` carries content identity - the TYPE half of the test, asked
    /// FIRST, before any key is looked at, so an event of any other type can never be
    /// suppressed however its metadata happens to be spelled.
    pub fn covers(&self, type_: &str) -> bool {
        self.types.iter().any(|t| t == type_)
    }

    /// WHERE `key`'s subject prefix and content generation lie within it, or `None`
    /// when the key is not of the configured content-key shape (in which case it names
    /// no generation and its append is never suppressed - the fail-safe direction).
    ///
    /// Every range the policy hands back is VALIDATED here, once, so no adapter has to
    /// re-derive the checks and none can skip them. A split is well formed only when:
    ///
    /// - the subject STARTS THE KEY (`subject.start == 0`). "Subject prefix" is not a
    ///   description, it is the property a store's range seek rests on: every key
    ///   naming one subject is exactly the keys beginning with it, which is what turns
    ///   "this subject's history" into one bounded range instead of a scan;
    /// - the generation lies at or after the subject's end and within the key;
    /// - both ranges are non-inverted and land on character boundaries, so slicing
    ///   them can never panic on a multi-byte key.
    ///
    /// A policy that breaks any of them names no generation, and an event whose key
    /// names no generation appends. A misconfigured composition root therefore
    /// DEGRADES to an unguarded store; it can never drop a fact.
    pub fn split_of(&self, key: &str) -> Option<(Range<usize>, Range<usize>)> {
        let (subject, generation) = (self.split)(key)?;
        let well_formed = subject.start == 0
            && subject.end <= generation.start
            && generation.start <= generation.end
            && generation.end <= key.len()
            && key.is_char_boundary(subject.end)
            && key.is_char_boundary(generation.start)
            && key.is_char_boundary(generation.end);
        well_formed.then_some((subject, generation))
    }

    /// [`ContentIdentity::split_of`] as the two slices themselves - borrows INTO `key`
    /// by construction, because they are cut from validated ranges rather than handed
    /// over by the policy.
    pub fn subject_of<'k>(&self, key: &'k str) -> Option<(&'k str, &'k str)> {
        let (subject, generation) = self.split_of(key)?;
        Some((&key[subject], &key[generation]))
    }
}

impl std::fmt::Debug for ContentIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContentIdentity")
            .field("meta_key", &self.meta_key)
            .field("types", &self.types)
            .field("reasserting", &self.reasserting)
            .finish_non_exhaustive()
    }
}

/// The metadata key an adapter stamps on an event it wrote while its content-identity
/// guard was NOT JUDGING - the guard's own degradation, recorded in the log it guards.
///
/// A guard that has stopped defending has to SAY SO, and this is the one place it can
/// say it durably. Stderr is not a record: the process that degrades is usually a
/// short-lived one whose output nobody is reading, and the symptom (a log growing
/// again) shows up days later in a different process. So the fact is written where
/// every other fact this system reasons about is written - into the log - and it is
/// written WITHOUT a new event type or a new serialized form: it rides as one extra
/// metadata pair on events the append was already writing, which `rigger validate`,
/// any read of the store, and any operator with a SQL prompt can see.
///
/// It is stamped ONLY on events of a type the guard COVERS - the derived-index types
/// the policy names - so a domain event is never rewritten by a store that merely
/// happened to be unhealthy while it landed.
pub const META_GUARD_DEGRADED: &str = "content_guard_degraded";

/// [`META_GUARD_DEGRADED`]: the guard had no usable content-key index, so it judged
/// nothing and every event of a covered type in that append was written through.
pub const GUARD_DEGRADED_NO_INDEX: &str = "no-index";

/// [`META_GUARD_DEGRADED`]: a subject's latest-generation walk exceeded its step
/// budget, so its current generation was UNDETERMINED and nothing was suppressed
/// against it.
pub const GUARD_DEGRADED_UNDETERMINED: &str = "generations-exceeded";

#[derive(Debug, Error)]
pub enum Error {
    #[error("event store: concurrency conflict on stream {stream:?}: expected {expected:?}, actual revision {actual}")]
    Conflict {
        stream: String,
        expected: ExpectedRevision,
        actual: Revision,
    },
    #[error("event store: {0}")]
    Backend(String),
}

/// A catch-up subscription: it replays the existing events from a position, then
/// streams new ones live, until it is dropped. Adapters feed it from a background
/// thread; callers consume it with the recv methods and check [`Subscription::err`]
/// for a terminal error after the stream ends.
pub struct Subscription {
    rx: Receiver<Event>,
    err: Arc<Mutex<Option<String>>>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Subscription {
    /// Build a subscription from a backend's event channel, its terminal-error
    /// cell, its stop flag, and the thread feeding it.
    pub fn new(
        rx: Receiver<Event>,
        err: Arc<Mutex<Option<String>>>,
        stop: Arc<AtomicBool>,
        handle: JoinHandle<()>,
    ) -> Self {
        Subscription {
            rx,
            err,
            stop,
            handle: Some(handle),
        }
    }

    /// Block for the next event, or None once the feeding thread has stopped.
    pub fn recv(&self) -> Option<Event> {
        self.rx.recv().ok()
    }

    /// Block up to `timeout` for the next event.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Event> {
        self.rx.recv_timeout(timeout).ok()
    }

    /// Take the next event if one is ready, without blocking.
    pub fn try_recv(&self) -> Option<Event> {
        self.rx.try_recv().ok()
    }

    /// The terminal error, if the feeding thread ended in one.
    pub fn err(&self) -> Option<String> {
        self.err.lock().unwrap().clone()
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// EventStore is the append-only, bi-temporal log port (KurrentDB-shaped).
/// Implementations are safe to share across threads.
///
/// # The `from` boundary convention
///
/// Every read and subscription takes a `from` cursor. The boundary is the same
/// for the read and the subscription that share a scope, so a catch-up
/// subscription and a read from the same `from` replay exactly the same set (no
/// dropped or duplicated boundary event):
///
/// - **Stream-scoped** ([`read_stream`](EventStore::read_stream),
///   [`subscribe_stream`](EventStore::subscribe_stream)): `from` is a per-stream
///   revision and is **inclusive**. `from == 0` includes revision 0 (the first
///   event); resuming from the revision of the last event you saw re-delivers
///   that event.
/// - **`$all`-scoped** ([`read_all`](EventStore::read_all),
///   [`subscribe_all`](EventStore::subscribe_all)): `from` is a global
///   [`Position`] and is **exclusive**. `from == 0` includes every event;
///   resuming from the position of the last event you processed delivers only
///   what came after it - the natural checkpoint shape (store the last handled
///   position, resume from it, never see it twice).
///
/// Adapters must honor this regardless of the backend's native boundary.
/// KurrentDB's `read_*`-from-a-position is inclusive while its
/// `subscribe_*`-from-a-position is exclusive; its adapter normalizes both onto
/// the convention above.
pub trait EventStore: Send + Sync {
    /// Append events to the end of a stream under an optimistic-concurrency
    /// expectation, reporting what was ACTUALLY written. A failed expectation yields
    /// [`Error::Conflict`] carrying the stream's actual current revision.
    ///
    /// # The honesty obligation
    ///
    /// The returned [`Appended`] carries one slot per event handed in, in input order,
    /// and every reported position is one the store ITSELF issued - never arithmetic
    /// an adapter invented. This is a PORT obligation every adapter owes, pinned by
    /// the backend-agnostic contract suite, because it is what lets a caller fold what
    /// it appended at the positions the log actually holds it at. An adapter that
    /// cannot answer where an event landed reports an error, never a guess.
    ///
    /// Reported positions are DISTINCT and strictly increasing within one append. They
    /// are NOT promised to be consecutive: a backend whose global position is a byte
    /// offset satisfies this port and leaves gaps.
    ///
    /// An append of no events writes nothing and reports an empty [`Appended`].
    ///
    /// A store may write FEWER events than it was handed when a
    /// [`ContentIdentity`] policy is configured and an event is already recorded
    /// under that policy (see [`sqlite::Store::with_content_identity`]); the
    /// suppressed events report `None` and consume no per-stream revision, so the
    /// stream advances by exactly the events written. Suppression is confined to the
    /// configured types: every other event appends per-append, so two identical
    /// domain events still write two rows.
    fn append(
        &self,
        stream: &str,
        expected: ExpectedRevision,
        events: &[Event],
    ) -> Result<Appended, Error>;

    /// Read one stream's events from a per-stream revision (**inclusive**), in a
    /// direction. Backward reads return the same set as a forward read from
    /// `from`, reversed (direction controls order, not the boundary).
    fn read_stream(
        &self,
        stream: &str,
        from: Revision,
        dir: Direction,
    ) -> Result<Vec<Event>, Error>;

    /// Read the global log from a global position (**exclusive**), in a
    /// direction, filtered. Backward reads return the same set as a forward read
    /// from `from`, reversed (direction controls order, not the boundary).
    fn read_all(
        &self,
        from: Position,
        dir: Direction,
        filter: &Filter,
    ) -> Result<Vec<Event>, Error>;

    /// Open a catch-up subscription over the global log from a position
    /// (**exclusive**): it replays the matching events after `from` in order,
    /// then delivers new ones live.
    fn subscribe_all(&self, from: Position, filter: &Filter) -> Result<Subscription, Error>;

    /// Open a catch-up subscription over one stream from a revision
    /// (**inclusive**): it replays that stream's events from `from` onward, then
    /// delivers new ones live.
    fn subscribe_stream(&self, stream: &str, from: Revision) -> Result<Subscription, Error>;
}

/// The marker that replaces a redacted credential, so a scrubbed connection string reads as
/// deliberately redacted (a human sees the credentials were removed) rather than silently
/// mangled or merely absent.
const REDACTED: &str = "<redacted>";

/// Redact the credential (userinfo) portion of every URL in `s` (§48, secrets discipline). A
/// connection string is a SECRET wherever it appears: any error, log line, or status output that
/// would echo it must scrub the `user:password@` that sits between `scheme://` and the host. The
/// scheme and host still print (they name WHICH server, which is useful in an error), but the
/// userinfo is replaced by the [`REDACTED`] marker and never reaches an output path.
///
/// This is the SINGLE redaction authority: every site that would surface a connection string in
/// user-facing text passes through here, so a credential can never leak from one forgotten branch.
/// It operates purely on the message text and never on the connection string handed to the client,
/// so verbatim pass-through to the backend is untouched.
///
/// A string with no URL userinfo returns unchanged. Redaction is confined to the URL AUTHORITY
/// (`[userinfo@]host[:port]`, between `scheme://` and the path/query/fragment): an `@` in a path or
/// query - not a credential separator - is left alone, and a userinfo with an embedded `:` (a
/// `user:password` pair) is scrubbed whole (the host begins at the last `@` of the authority). A
/// `/`, `?`, or `#` normally ENDS the authority, but a password may carry one of those chars
/// unencoded (malformed per RFC 3986, yet handed to the parser verbatim), so such a char BEFORE the
/// credential's terminating `@` does not stop the scrub - the whole userinfo is still removed.
pub fn redact_conn(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(scheme_end) = rest.find("://") {
        // Everything up to and including the "://" separator prints verbatim (scheme is not secret).
        let after = scheme_end + "://".len();
        out.push_str(&rest[..after]);
        let tail = &rest[after..];
        // Bound the current URL at the NEXT `://` so one authority never runs into the following
        // scheme. This is load-bearing: without it, a message that names two servers back-to-back
        // with no `/`, `?`, or `#` between them (`...@h1 kurrentdb://u2:p2@h2`, or comma/paren-
        // delimited `...@h1,kurrentdb://u2:p2@h2`) lets the scan swallow the next scheme, so the
        // leftover `//user:pass@host` no longer begins with a `scheme://` and is never re-scanned -
        // leaking the second credential verbatim. Bounding here scrubs EVERY URL's userinfo whatever
        // the delimiter between URLs (one server named twice - the parse error's double-embed - as
        // well as many named once).
        let seg_end = tail.find("://").unwrap_or(tail.len());
        let seg = &tail[..seg_end];
        let auth_end = authority_end(seg);
        let authority = &seg[..auth_end];
        // Userinfo is separated from the host by the LAST `@` inside the authority (a bare `@` never
        // appears in a host, and userinfo cannot contain an unencoded `@`), so anything before it is
        // the credential and is replaced by the marker; the host and beyond print unchanged.
        match authority.rfind('@') {
            Some(at) => {
                out.push_str(REDACTED);
                out.push_str(&authority[at..]); // includes the '@' and the host
            }
            None => out.push_str(authority),
        }
        rest = &tail[auth_end..];
    }
    out.push_str(rest);
    out
}

/// The byte offset in `seg` (one URL's tail, already bounded so it never reaches the next URL's
/// `scheme://`) at which the AUTHORITY ends - i.e. where `host[:port]` gives way to the path, query,
/// or fragment. The authority is `[userinfo@]host[:port]`.
///
/// The authority normally ends at the first `/`, `?`, or `#`. The subtlety this function exists for:
/// a password may carry one of those chars UNENCODED, and such a char sits inside the userinfo,
/// BEFORE the credential's terminating `@`. Ending the authority at that first delimiter would slice
/// the credential off before its `@`, so [`redact_conn`]'s `rfind('@')` would find nothing and print
/// the `user:pass` verbatim - a leak. So when the first delimiter is NOT preceded by a completed
/// authority, the credential runs on to its terminating `@` and the authority ends only at the first
/// delimiter AFTER the host.
///
/// The genuine-path/query `@` case (an `@` that a caller legitimately put in a path or query, which
/// must NOT be scrubbed) is told apart by exactly this: it follows a delimiter that DID close a
/// well-formed `host[:port]`, so that authority already ended and the `@` is post-authority.
fn authority_end(seg: &str) -> usize {
    let Some(delim) = seg.find(['/', '?', '#']) else {
        return seg.len(); // no path/query/fragment: the whole segment is the authority
    };
    let head = &seg[..delim];
    // If the userinfo `@` already precedes the delimiter (the well-formed case), or the text before
    // the delimiter is a complete `host[:port]` (the delimiter genuinely starts the path/query/
    // fragment), the authority ends right at the delimiter and any later `@` is post-authority.
    if head.contains('@') || is_host_port(head) {
        return delim;
    }
    // Otherwise the delimiter is an unencoded reserved char INSIDE the userinfo. The credential runs
    // on to its terminating `@`; the authority then ends at the first delimiter after the host (the
    // host carries no `@`, so `rfind('@')` on the returned slice still isolates the whole userinfo).
    // If there is no such `@`, the pre-delimiter text was not a credential after all - fall back to
    // the delimiter, leaving the segment untouched.
    match seg[delim..].find('@') {
        Some(rel_at) => {
            let at = delim + rel_at;
            seg[at..]
                .find(['/', '?', '#'])
                .map_or(seg.len(), |rel| at + rel)
        }
        None => delim,
    }
}

/// True when `s` is a complete URL authority with NO userinfo: a bare `host`, or `host:port` with a
/// non-empty all-digit port. Used to tell a `/`, `?`, or `#` that genuinely closes the authority
/// (starting the path/query/fragment) from one buried unencoded inside a credential's password.
fn is_host_port(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    match s.rsplit_once(':') {
        // `host:port` - a port must be present and all ASCII digits (a non-numeric "port" like the
        // `pass` in `user:pass` is what marks this as a credential, not a host).
        Some((host, port)) => {
            !host.is_empty() && !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit())
        }
        // A bare reg-name / IP host with no port.
        None => true,
    }
}

/// Reduce a connection string to a CREDENTIAL-FREE endpoint label safe to PERSIST and print: keep
/// the scheme and `host[:port]`, DROP any `[userinfo@]` credential AND any `/path`, `?query`, or
/// `#fragment` (a connection string can smuggle a credential in the userinfo OR the query). So
/// `kurrentdb://admin:secret@db.example:2113?tls=true` reduces to `kurrentdb://db.example:2113`.
///
/// This is the discovery-metadata sibling of [`redact_conn`] and shares its ONE hardened authority
/// parse ([`authority_end`]/[`is_host_port`]) - it is NOT a second redaction authority. Where
/// [`redact_conn`] masks the userinfo IN PLACE for a human-facing message (keeping the host and the
/// query), `endpoint_label` STRIPS the userinfo (and the query/path/fragment) entirely, for a value
/// that is written to disk (the instance registry's credential-free store identity, §50).
///
/// Routing through [`authority_end`] is load-bearing for the secrets invariant: a password may carry
/// an unencoded `/`, `?`, or `#` BEFORE its terminating `@` (malformed per RFC 3986, yet handed to
/// the parser verbatim). A naive parse that ends the authority at that first delimiter would slice
/// the credential off before its `@`, so `rfind('@')` would find nothing and the `user:pass` head
/// would land in the persisted label - a leak. The shared parse runs the credential on to its
/// terminating `@` instead, so `kurrentdb://user:pa/ss@host:2113` reduces to `kurrentdb://host:2113`,
/// never `kurrentdb://user:pa`. Pure, so the "the registry never holds a credential" invariant is
/// unit-tested with no store.
pub fn endpoint_label(conn: &str) -> String {
    let conn = conn.trim();
    // Split off the scheme, preserving it in the output (it names the backend). A conn with no
    // `scheme://` is treated as a bare authority so a malformed `user:pass@host` is still scrubbed.
    let (scheme, tail) = match conn.find("://") {
        Some(i) => (&conn[..i + "://".len()], &conn[i + "://".len()..]),
        None => ("", conn),
    };
    // The AUTHORITY, bounded by the shared hardened parse (which keeps a credential's unencoded
    // delimiter on the credential side of its terminating `@`), then with any `[userinfo@]` dropped:
    // the host begins at the LAST `@` of the authority (userinfo cannot contain an unencoded `@`).
    let authority = &tail[..authority_end(tail)];
    let host_port = match authority.rfind('@') {
        Some(at) => &authority[at + 1..],
        None => authority,
    };
    format!("{scheme}{host_port}")
}

#[cfg(test)]
mod endpoint_label_tests {
    use super::endpoint_label;

    #[test]
    fn strips_userinfo_and_query_keeps_scheme_host_port() {
        assert_eq!(
            endpoint_label("kurrentdb://admin:secret@db.example:2113?tls=true"),
            "kurrentdb://db.example:2113",
            "userinfo AND query are stripped; only scheme+host:port survives"
        );
    }

    #[test]
    fn strips_a_bare_user_with_no_password() {
        assert_eq!(
            endpoint_label("esdb+discover://user@cluster.internal:2113"),
            "esdb+discover://cluster.internal:2113"
        );
    }

    #[test]
    fn an_already_credential_free_endpoint_is_unchanged() {
        assert_eq!(
            endpoint_label("kurrentdb://db.example:2113"),
            "kurrentdb://db.example:2113"
        );
    }

    #[test]
    fn a_credential_smuggled_after_the_path_is_dropped_with_the_path() {
        assert_eq!(
            endpoint_label("kurrentdb://host:2113/stream?user=u&password=p"),
            "kurrentdb://host:2113",
            "a `?user=&password=` query is dropped with the path"
        );
    }

    /// The GROUND-1 regression: a password carrying an unencoded delimiter BEFORE its terminating
    /// `@`. A naive parse ends the authority at the first `/`, so the `@` (and thus the host) is
    /// lost and the `user:pa` HEAD of the credential is what gets persisted - a leak. The shared
    /// hardened `authority_end` runs the credential on to its `@`, so the persisted label is the
    /// pure host and NO credential fragment survives.
    #[test]
    fn a_delimiter_inside_the_userinfo_never_leaks_the_credential_head() {
        assert_eq!(
            endpoint_label("kurrentdb://user:pa/ss@host:2113"),
            "kurrentdb://host:2113",
            "an unencoded `/` inside the password must not slice the authority before the `@`"
        );
        assert_eq!(
            endpoint_label("kurrentdb://user:pa?ss@host:2113"),
            "kurrentdb://host:2113",
            "an unencoded `?` inside the password is handled the same way"
        );
        // A no-scheme, malformed conn that still hides a credential is scrubbed to the bare host.
        assert_eq!(
            endpoint_label("user:pa/ss@host:2113"),
            "host:2113",
            "no scheme is no excuse to leak: a bare `user:pass@host` is still stripped"
        );
    }

    /// The persisted label must NEVER contain a credential fragment, whatever the (malformed) shape.
    #[test]
    fn no_credential_fragment_ever_survives() {
        for conn in [
            "kurrentdb://admin:hunter2@db.example:2113?tls=true",
            "kurrentdb://admin:hun/ter2@db.example:2113",
            "esdb+discover://admin@cluster.internal:2113",
            "kurrentdb://host:2113/s?user=admin&password=hunter2",
        ] {
            let label = endpoint_label(conn);
            assert!(
                !label.contains("admin") && !label.contains("hunter2") && !label.contains("hun"),
                "no credential fragment may reach the persisted label for {conn:?}; got {label}"
            );
        }
    }
}

#[cfg(test)]
mod redact_tests {
    use super::redact_conn;

    #[test]
    fn strips_user_and_password_but_keeps_scheme_host_and_query() {
        let redacted = redact_conn("kurrentdb://myuser:supersecret@db.internal:2113?tls=true");
        assert!(
            !redacted.contains("supersecret") && !redacted.contains("myuser"),
            "userinfo must never survive redaction: {redacted}"
        );
        assert_eq!(
            redacted, "kurrentdb://<redacted>@db.internal:2113?tls=true",
            "scheme, host, port, and query print; only the credential is scrubbed"
        );
    }

    #[test]
    fn strips_a_userinfo_with_no_password() {
        assert_eq!(
            redact_conn("kurrentdb://alice@db.internal:2113"),
            "kurrentdb://<redacted>@db.internal:2113"
        );
    }

    #[test]
    fn leaves_a_conn_with_no_userinfo_unchanged() {
        let conn = "kurrentdb://127.0.0.1:2113?tls=false";
        assert_eq!(
            redact_conn(conn),
            conn,
            "no credential means nothing to scrub"
        );
    }

    #[test]
    fn an_at_sign_in_the_query_is_not_a_credential() {
        // The `@` sits in the query, not the authority, so it is NOT a userinfo separator.
        let conn = "kurrentdb://db.internal:2113?user=a@b";
        assert_eq!(redact_conn(conn), conn);
    }

    #[test]
    fn redacts_a_conn_embedded_in_a_longer_error_message() {
        let msg = "kurrentdb: connect to kurrentdb://joe:pw@10.0.0.5:2113?tls=false: timed out";
        let redacted = redact_conn(msg);
        assert!(
            !redacted.contains("joe") && !redacted.contains("pw@"),
            "the credential inside a wrapping message must be scrubbed: {redacted}"
        );
        assert!(
            redacted.contains("10.0.0.5:2113") && redacted.contains("timed out"),
            "the host and the surrounding message text still print: {redacted}"
        );
    }

    #[test]
    fn plain_text_with_no_url_is_untouched() {
        assert_eq!(
            redact_conn("no rigger store found"),
            "no rigger store found"
        );
    }
}

/// A port that ACCEPTS every append and reports writing nothing - the one answer every
/// single-event seam has to surface rather than absorb. It lives here, beside the
/// accessor that decides what that answer means, so each seam's test holds it to the SAME
/// double instead of to a local one that could drift into a friendlier shape.
///
/// Reads answer EMPTY rather than failing: a seam that reads before it appends (a
/// compare-and-append) must reach its append to be tested at all.
#[cfg(test)]
pub(crate) struct SilentStore;

#[cfg(test)]
impl EventStore for SilentStore {
    fn append(
        &self,
        _stream: &str,
        _expected: ExpectedRevision,
        events: &[Event],
    ) -> Result<Appended, Error> {
        Ok(Appended::from_placements(vec![None; events.len()]))
    }
    fn read_stream(
        &self,
        _stream: &str,
        _from: Revision,
        _dir: Direction,
    ) -> Result<Vec<Event>, Error> {
        Ok(Vec::new())
    }
    fn read_all(
        &self,
        _from: Position,
        _dir: Direction,
        _filter: &Filter,
    ) -> Result<Vec<Event>, Error> {
        Ok(Vec::new())
    }
    fn subscribe_all(&self, _from: Position, _filter: &Filter) -> Result<Subscription, Error> {
        Err(Error::Backend(
            "the silent double answers appends only".into(),
        ))
    }
    fn subscribe_stream(&self, _stream: &str, _from: Revision) -> Result<Subscription, Error> {
        Err(Error::Backend(
            "the silent double answers appends only".into(),
        ))
    }
}

/// THE ONE MEANING OF AN ABSENCE ON A SINGLE-EVENT APPEND, tested where it is decided.
///
/// Every seam that appends exactly one event reads its report through `Appended::one`, so
/// these tests pin the answer all of them share. Whether a seam still ASKS it is a
/// different question, and each seam pins that itself.
#[cfg(test)]
mod appended_one_tests {
    use super::{Appended, Error};

    #[test]
    fn a_written_event_yields_the_position_the_store_issued() {
        let report = Appended::all(vec![41]);
        assert_eq!(
            report
                .one("the decision of u1")
                .expect("the store wrote it"),
            41,
            "the accessor hands back the store's own position, never a derived one"
        );
    }

    #[test]
    fn a_store_that_wrote_nothing_is_an_error_naming_what_was_lost() {
        let report = Appended::from_placements(vec![None]);
        let err = report
            .one("the decision of u1")
            .expect_err("an event nobody can locate has not been recorded");
        let message = err.to_string();
        assert!(
            matches!(err, Error::Backend(_)),
            "a port that accepted the append and wrote nothing is a backend failure, not a \
             concurrency conflict: {message}"
        );
        assert!(
            message.contains("nothing"),
            "the message says the store wrote nothing: {message}"
        );
        assert!(
            message.contains("the decision of u1"),
            "and names what the caller was recording, so the loss is identifiable: {message}"
        );
    }

    #[test]
    fn an_empty_report_is_the_same_failure_and_never_a_position() {
        let err = Appended::default()
            .one("the decision of u1")
            .expect_err("a report with no slot at all placed no event either");
        assert!(
            err.to_string().contains("nothing"),
            "an empty report and a suppressed slot are the same answer to a one-event \
             caller: no position was issued"
        );
    }

    #[test]
    fn a_report_answering_more_than_one_event_yields_no_position() {
        let err = Appended::all(vec![7, 9])
            .one("the decision of u1")
            .expect_err("a two-event report does not answer a one-event caller");
        let message = err.to_string();
        assert!(
            !message.contains('9'),
            "and it must not silently hand back the last of several positions as though it \
             were the one: {message}"
        );
    }
}
