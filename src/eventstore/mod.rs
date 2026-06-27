//! The event store: an append-only log with optimistic concurrency, the source
//! of truth the ledger and context graph project from. `EventStore` is the port;
//! `sqlite` is the default adapter, shaped after KurrentDB so the embedded store
//! is a faithful stand-in for the server one.

pub mod namespace;
pub mod sqlite;

#[cfg(feature = "kurrentdb")]
pub mod kurrentdb;

#[cfg(test)]
pub mod contract;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};

use thiserror::Error;

/// A global ($all-order) position, assigned by the store on append.
pub type Position = u64;

/// An event in the log. `data` is the opaque (usually JSON) payload.
#[derive(Clone, Debug)]
pub struct Event {
    pub id: String,
    pub type_: String,
    pub data: Vec<u8>,
    pub recorded_at: SystemTime,
    pub position: Position,
}

impl Event {
    /// A new event with a fresh id and the current time; the store assigns the
    /// final position on append.
    pub fn new(type_: impl Into<String>, data: Vec<u8>) -> Self {
        Event {
            id: uuid::Uuid::new_v4().to_string(),
            type_: type_.into(),
            data,
            recorded_at: SystemTime::now(),
            position: 0,
        }
    }
}

/// The version a stream is expected to be at when appending (optimistic
/// concurrency): any version, no stream yet, or an exact event count.
#[derive(Clone, Copy, Debug)]
pub enum ExpectedRevision {
    Any,
    NoStream,
    Exact(u64),
}

/// Read direction over a stream or the global log.
#[derive(Clone, Copy, Debug)]
pub enum Direction {
    Forward,
    Backward,
}

/// A read/subscription filter over the global log.
#[derive(Clone, Debug, Default)]
pub struct Filter {
    pub stream_prefix: Option<String>,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("event store: wrong expected version for stream {stream:?}")]
    Conflict { stream: String },
    #[error("event store: {0}")]
    Backend(String),
}

/// A catch-up subscription: it replays the existing events from a position, then
/// streams new ones live, until it is dropped. Adapters feed it from a background
/// thread; callers consume it with the recv methods.
pub struct Subscription {
    rx: Receiver<Event>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Subscription {
    /// Build a subscription from a backend's event channel, its stop flag, and the
    /// thread feeding it.
    pub fn new(rx: Receiver<Event>, stop: Arc<AtomicBool>, handle: JoinHandle<()>) -> Self {
        Subscription {
            rx,
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
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// EventStore is the append-only log port (KurrentDB-shaped). Implementations are
/// safe to share across threads.
pub trait EventStore: Send + Sync {
    /// Append events to a stream under an optimistic-concurrency expectation,
    /// returning the last global position written. A failed expectation yields
    /// `Error::Conflict`.
    fn append(
        &self,
        stream: &str,
        expected: ExpectedRevision,
        events: &[Event],
    ) -> Result<Position, Error>;

    /// Read one stream's events from a global position, in a direction.
    fn read_stream(
        &self,
        stream: &str,
        from: Position,
        dir: Direction,
    ) -> Result<Vec<Event>, Error>;

    /// Read the global log from a position, in a direction, filtered.
    fn read_all(
        &self,
        from: Position,
        dir: Direction,
        filter: &Filter,
    ) -> Result<Vec<Event>, Error>;

    /// Open a catch-up subscription over the global log from a position: it
    /// replays the matching events in order, then delivers new ones live.
    fn subscribe_all(&self, from: Position, filter: &Filter) -> Result<Subscription, Error>;
}
