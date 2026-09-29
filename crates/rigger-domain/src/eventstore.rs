//! The event log's domain types and its port: the immutable `Event` fact, its ordering values,
//! the append report, the content-identity policy and the `EventStore` trait every backend
//! implements. The backends themselves live in the root crate's `eventstore` module.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
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

/// Which event types a [`EventStore::read_stream_typed`] hands back (spec 101): `Only` the named
/// types, or every type `Except` the named ones. The store answers either from its type index, so
/// an event the selection refuses is never materialized.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeSelection<'a> {
    Only(&'a [&'a str]),
    Except(&'a [&'a str]),
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
        let (id, now) = Self::mint();
        Event {
            id,
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

    /// A fresh identity for a new event: its random id, minted with real entropy, and its
    /// construction timestamp, read from the real clock (spec 93, criterion 1: THE CORE LANE
    /// IS PURE). This is the `store` half - the `core` lane's own stub is below.
    #[cfg(any(feature = "store", not(feature = "core")))]
    fn mint() -> (String, SystemTime) {
        (uuid::Uuid::new_v4().to_string(), SystemTime::now())
    }

    /// The pure `core` lane's identity stub. It has no entropy source at all (`uuid` is a
    /// banned import, and the wasm target the lane also builds for carries no `getrandom`
    /// backend without extra opt-in). A page constructs an `Event` only to hand its
    /// already-decoded JSON to `fold_push`/`fold_reset` (spec 93 criterion 2's ABI), which
    /// stamps nothing itself - the id it read off the wire is already the recorded one - so
    /// an empty id here is never presented as a real, storable identity; every real mint runs
    /// through the `store` arm above. And CONSTRAINTS WALK, Clock (spec 93): "the core has no
    /// `now()`, every age arrives as an input" - `SystemTime::now()` is itself a banned
    /// import, so the timestamp is the epoch rather than the clock; a caller that needs a
    /// real timestamp on a core-built `Event` sets it explicitly via
    /// [`Event::with_valid_from`], or reads it off the wire it decoded the event from.
    #[cfg(all(feature = "core", not(feature = "store")))]
    fn mint() -> (String, SystemTime) {
        (String::new(), SystemTime::UNIX_EPOCH)
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

    /// The payload decoded as a `T`, or `None` when it is not one - the sentinel arm that keeps
    /// a fold panic-free on a malformed, foreign or partial event.
    pub fn decode<T: serde::de::DeserializeOwned>(&self) -> Option<T> {
        serde_json::from_slice(&self.data).ok()
    }
}

/// `t` as signed nanoseconds since the Unix epoch - the integer a store persists a
/// [`Event::valid_from`] as. A time before the epoch encodes as `0`.
pub fn to_nanos(t: SystemTime) -> i64 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

/// The inverse of [`to_nanos`]: a persisted nanosecond count back to a time, clamping a
/// negative count to the epoch.
pub fn from_nanos(n: i64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_nanos(n.max(0) as u64)
}

/// What an [`EventStore::append`] actually wrote: ONE entry per event the store was
/// handed, in the order it was handed them. `Some(position)` is the global
/// [`Position`] the store ISSUED for that event; `None` is an event the store
/// recognised as already recorded and did not write.
///
/// The report exists because an append may write FEWER events than it was handed (an
/// adapter may recognise an event as already recorded), and a caller
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
        Self::from_placements(positions.into_iter().map(Some).collect())
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

/// The content-identity policy the store's derived-index maintenance (measurement and
/// compaction) runs under: WHICH event types carry content identity and WHERE an event
/// carries its content key.
///
/// This is CONFIGURATION, injected at the composition root, never vocabulary the store
/// owns: the event types whose payload is a re-derivable index of the project's own
/// sources are knowledge of the layer that derives them, and the store is the lower
/// port. Handing the policy in keeps the store free of any dependency on that layer.
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
#[derive(Clone, Debug)]
pub struct ContentIdentity {
    meta_key: String,
    types: Vec<String>,
    /// The covered types whose recordings RE-ASSERT, or `None` when this policy has
    /// never been told the partition. `None` is not "no type re-asserts": the two are
    /// different states on purpose, because a compaction cannot act correctly on the
    /// first and must say so rather than guess (see [`reasserts`](Self::reasserts)).
    reasserting: Option<Vec<String>>,
}

impl ContentIdentity {
    /// Build the policy. `meta_key` is the metadata key an identified event carries
    /// its content key under; `types` are the event types that carry content identity
    /// (every other type keeps per-append identity untouched).
    pub fn new(
        meta_key: impl Into<String>,
        types: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        ContentIdentity {
            meta_key: meta_key.into(),
            types: types.into_iter().map(Into::into).collect(),
            reasserting: None,
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

    /// Whether `type_` carries content identity: an event of any other type is never
    /// touched by derived-index maintenance, however its metadata happens to be spelled.
    pub fn covers(&self, type_: &str) -> bool {
        self.types.iter().any(|t| t == type_)
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("event store: concurrency conflict on stream {stream:?}: expected {expected:?}, actual revision {actual}")]
    Conflict {
        stream: String,
        expected: ExpectedRevision,
        actual: Revision,
    },
    /// Spec 71 - APPEND REFUSES DISORDER. The store's position-order cursor for
    /// `stream` (the revision its own last-written row holds) sits at or below a
    /// revision the stream ALREADY records elsewhere: appending `attempted` would land
    /// it at or below `recorded`, which never happens through this port alone (every
    /// write it issues strictly extends both orders together) and is the exact
    /// signature a stale writer leaves behind after a compaction reissues a revision
    /// hole. Refusing here, before the write, is what keeps that signature from
    /// silently compounding and turns what would otherwise be a bare
    /// `UNIQUE(stream, revision)` failure into a named, actionable one.
    #[error(
        "event store: append to stream {stream:?} refused: revision {attempted} would sort at \
         or below revision {recorded} already recorded for this stream - the writer is stale, \
         most likely one running from before a compaction reissued a revision hole; reinstall \
         or restart it"
    )]
    OutOfOrder {
        stream: String,
        attempted: Revision,
        recorded: Revision,
    },
    #[error("event store: {0}")]
    Backend(String),
}

/// A catch-up subscription: it replays the existing events from a position, then
/// streams new ones live, until it is dropped. Adapters feed it from a background
/// thread; callers consume it with the recv methods and check [`Subscription::err`]
/// for a terminal error after the stream ends.
pub struct Subscription {
    // Declared first so it drops first: the feeding thread is stopped and joined before the
    // channel it sends on is torn down.
    _feeder: StoppableThread,
    rx: Receiver<Event>,
    err: Arc<Mutex<Option<String>>>,
}

/// A background thread that runs until its stop flag is raised: dropping this handle raises
/// the flag and joins the thread, so the thread never outlives its owner.
pub struct StoppableThread {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl StoppableThread {
    /// Own `handle`, the thread that polls `stop` and ends once it is raised.
    pub fn new(stop: Arc<AtomicBool>, handle: JoinHandle<()>) -> Self {
        StoppableThread {
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for StoppableThread {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
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
            _feeder: StoppableThread::new(stop, handle),
            rx,
            err,
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

    /// This subscription's events, each passed through `f` in delivery order, as a new
    /// subscription: the one relay a decorator reshapes or observes a backend's deliveries
    /// through. The relay owns this subscription, so dropping the result stops both; this
    /// subscription's terminal error carries over once its events are relayed.
    pub fn map<F>(self, mut f: F) -> Subscription
    where
        F: FnMut(Event) -> Event + Send + 'static,
    {
        let (tx, rx) = channel();
        let err = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let stop_relay = Arc::clone(&stop);
        let err_relay = Arc::clone(&err);
        let handle = std::thread::spawn(move || {
            while !stop_relay.load(Ordering::Relaxed) {
                match self.recv_timeout(Duration::from_millis(50)) {
                    Some(e) => {
                        if tx.send(f(e)).is_err() {
                            return;
                        }
                    }
                    None => {
                        if let Some(msg) = self.err() {
                            *err_relay.lock().unwrap() = Some(msg);
                            return;
                        }
                        // a quiet timeout: the inner is still live; re-check stop
                    }
                }
            }
        });
        Subscription::new(rx, err, stop, handle)
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
    /// A store may write FEWER events than it was handed when it recognises an event as
    /// already recorded; the suppressed events report `None` and consume no per-stream
    /// revision, so the stream advances by exactly the events written.
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

    /// The per-stream revision of the NEWEST event of type `event_type` on `stream`, or `None`
    /// when the stream holds no such event (or does not exist). The answer is the inclusive
    /// `from` [`read_stream`](EventStore::read_stream) takes, so a read from it starts AT that
    /// event: with `RunStarted` it is the current run's boundary (spec 101).
    ///
    /// A backend answers from its own index, or by a backward read that stops at the first
    /// match - never by reading the stream forward, and never by materializing it.
    fn last_position(&self, stream: &str, event_type: &str) -> Result<Option<Revision>, Error>;

    /// Read one stream forward from the event at per-stream revision `from` (**inclusive**),
    /// handing back only the events `selection` admits, in log (position) order (spec 101). The
    /// read is anchored on that EVENT: it hands back the event and everything the log recorded
    /// on this stream after it, which on a well-formed stream is exactly the events at revision
    /// `from` and above; a `from` of 0 (or below) reads from the stream's start, and a `from`
    /// past the stream's last revision reads nothing. A backend answers the selection from its
    /// own type index or server-side filter, so a refused event is never materialized for the
    /// caller.
    fn read_stream_typed(
        &self,
        stream: &str,
        from: Revision,
        selection: TypeSelection,
    ) -> Result<Vec<Event>, Error>;
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

#[cfg(test)]
mod expected_revision_tests {
    use super::{ExpectedRevision, NO_STREAM};

    #[test]
    fn an_expectation_admits_exactly_the_last_revisions_it_names() {
        for last in [NO_STREAM, 0, 3] {
            assert!(ExpectedRevision::Any.admits(last), "Any admits {last}");
        }
        assert!(ExpectedRevision::NoStream.admits(NO_STREAM));
        assert!(!ExpectedRevision::NoStream.admits(0));
        assert!(ExpectedRevision::Exact(3).admits(3));
        assert!(!ExpectedRevision::Exact(3).admits(2) && !ExpectedRevision::Exact(3).admits(4));
    }
}

#[cfg(test)]
mod subscription_map_tests {
    use super::{Event, Subscription};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::channel;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    /// A subscription fed `types` in order, ending in the terminal error `end` once they are
    /// sent - or, with no error, kept open until it is stopped.
    fn fed(types: &[&str], end: Option<&str>) -> Subscription {
        let (tx, rx) = channel();
        let err = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let (err_t, stop_t) = (Arc::clone(&err), Arc::clone(&stop));
        let types: Vec<String> = types.iter().map(|t| t.to_string()).collect();
        let end = end.map(str::to_string);
        let handle = std::thread::spawn(move || {
            for t in types {
                tx.send(Event::new(&t, b"{}".to_vec())).unwrap();
            }
            if let Some(msg) = end {
                *err_t.lock().unwrap() = Some(msg);
                return;
            }
            while !stop_t.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        Subscription::new(rx, err, stop, handle)
    }

    /// Every delivered event passes through the map, in order, and the inner subscription's
    /// terminal error carries over to the mapped one once its events are drained.
    #[test]
    fn a_mapped_subscription_delivers_each_event_reshaped_in_order_then_the_inner_error() {
        let mapped = fed(&["a", "b", "c"], Some("gone")).map(|mut e| {
            e.type_.push('!');
            e
        });
        let got: Vec<String> = (0..3)
            .map(|_| mapped.recv_timeout(Duration::from_secs(10)).unwrap().type_)
            .collect();
        assert_eq!(got, ["a!", "b!", "c!"]);
        let deadline = Instant::now() + Duration::from_secs(10);
        while mapped.err().is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(mapped.err(), Some("gone".to_string()));
        assert!(mapped.recv_timeout(Duration::from_millis(200)).is_none());
    }

    /// A live inner subscription with nothing to deliver keeps the mapped one open and error-free,
    /// and dropping the mapped subscription stops both relay and inner (the drop returns).
    #[test]
    fn a_quiet_live_inner_keeps_the_mapped_subscription_open_until_it_is_dropped() {
        let mapped = fed(&[], None).map(|e| e);
        assert!(mapped.recv_timeout(Duration::from_millis(200)).is_none());
        assert_eq!(mapped.err(), None);
        drop(mapped);
    }
}
