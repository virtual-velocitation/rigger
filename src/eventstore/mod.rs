//! The append-only, bi-temporal event store: an immutable log of facts the ledger
//! and context graph are projected from. `EventStore` is the port; `sqlite` is the
//! default adapter. The trait mirrors KurrentDB's primitives - per-stream append
//! with optimistic concurrency, a global $all order, per-stream revisions, and
//! catch-up subscriptions - so a backend swaps without changing the rest of Rigger.

pub mod namespace;
pub mod sqlite;

#[cfg(feature = "kurrentdb")]
pub mod kurrentdb;

#[cfg(test)]
pub mod contract;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};

use thiserror::Error;

/// Position is an event's place in the global $all order: 1-based, store-assigned,
/// only ever increasing.
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
pub trait EventStore: Send + Sync {
    /// Append events to the end of a stream under an optimistic-concurrency
    /// expectation, returning the global position of the last event written. A
    /// failed expectation yields [`Error::Conflict`] carrying the stream's actual
    /// current revision.
    fn append(
        &self,
        stream: &str,
        expected: ExpectedRevision,
        events: &[Event],
    ) -> Result<Position, Error>;

    /// Read one stream's events from a per-stream revision, in a direction.
    fn read_stream(
        &self,
        stream: &str,
        from: Revision,
        dir: Direction,
    ) -> Result<Vec<Event>, Error>;

    /// Read the global log from a global position, in a direction, filtered.
    fn read_all(
        &self,
        from: Position,
        dir: Direction,
        filter: &Filter,
    ) -> Result<Vec<Event>, Error>;

    /// Open a catch-up subscription over the global log from a position: it
    /// replays the matching events in order, then delivers new ones live.
    fn subscribe_all(&self, from: Position, filter: &Filter) -> Result<Subscription, Error>;

    /// Open a catch-up subscription over one stream from a revision: it replays
    /// that stream's events from `from`, then delivers new ones live.
    fn subscribe_stream(&self, stream: &str, from: Revision) -> Result<Subscription, Error>;
}
