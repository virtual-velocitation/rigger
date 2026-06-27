//! The event store: an append-only log with optimistic concurrency, the source
//! of truth the ledger and context graph project from. `EventStore` is the port;
//! `sqlite` is the default adapter, shaped after KurrentDB so the embedded store
//! is a faithful stand-in for the server one.

pub mod sqlite;

use std::time::SystemTime;
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
}
