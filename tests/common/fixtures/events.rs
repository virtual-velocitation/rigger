//! Event fixtures.

use rigger::eventstore::{
    Appended, Direction, Error, Event, EventStore, ExpectedRevision, Filter, Position, Revision,
    Subscription,
};

/// An event of type `type_` whose payload is the UTF-8 bytes of `json`.
pub fn ev(type_: &str, json: &str) -> Event {
    Event::new(type_, json.as_bytes().to_vec())
}

/// An event of type `type_` carrying `payload` serialized, stamped at log position `pos`.
pub fn ev_at(pos: u64, type_: &str, payload: serde_json::Value) -> Event {
    let mut e = Event::new(type_, serde_json::to_vec(&payload).unwrap());
    e.position = pos;
    e
}

/// How many of `events` are of type `type_`.
pub fn count_of_type(events: &[Event], type_: &str) -> usize {
    events.iter().filter(|e| e.type_ == type_).count()
}

/// `events` stamped with 1-based log positions, as the store stamps them on append, so a
/// position-sensitive read (a cursor, `?since=`, a position-ordered fold) sees a realistic
/// monotonic stream.
pub fn positioned(mut events: Vec<Event>) -> Vec<Event> {
    for (i, e) in events.iter_mut().enumerate() {
        e.position = (i + 1) as u64;
    }
    events
}

/// The read, subscribe and boundary-lookup methods of an `EventStore` decorator that intercepts only
/// `append`, each forwarded unchanged to the decorator's `inner` store - expanded inside that
/// decorator's `impl EventStore` block.
#[macro_export]
macro_rules! delegate_event_store_reads {
    () => {
        fn read_stream(
            &self,
            stream: &str,
            from: rigger::eventstore::Revision,
            dir: rigger::eventstore::Direction,
        ) -> Result<Vec<rigger::eventstore::Event>, rigger::eventstore::Error> {
            self.inner.read_stream(stream, from, dir)
        }
        fn read_all(
            &self,
            from: rigger::eventstore::Position,
            dir: rigger::eventstore::Direction,
            filter: &rigger::eventstore::Filter,
        ) -> Result<Vec<rigger::eventstore::Event>, rigger::eventstore::Error> {
            self.inner.read_all(from, dir, filter)
        }
        fn subscribe_all(
            &self,
            from: rigger::eventstore::Position,
            filter: &rigger::eventstore::Filter,
        ) -> Result<rigger::eventstore::Subscription, rigger::eventstore::Error> {
            self.inner.subscribe_all(from, filter)
        }
        fn subscribe_stream(
            &self,
            stream: &str,
            from: rigger::eventstore::Revision,
        ) -> Result<rigger::eventstore::Subscription, rigger::eventstore::Error> {
            self.inner.subscribe_stream(stream, from)
        }
        fn last_position(
            &self,
            stream: &str,
            event_type: &str,
        ) -> Result<Option<rigger::eventstore::Revision>, rigger::eventstore::Error> {
            self.inner.last_position(stream, event_type)
        }
    };
}

/// A port that ACCEPTS every append and reports writing nothing - the one answer every
/// single-event seam has to surface rather than absorb. It lives with the shared fixtures,
/// one definition for every crate whose seams append through the port, so each seam's test
/// holds it to the SAME double instead of to a local one that could drift into a friendlier
/// shape.
///
/// Reads answer EMPTY rather than failing: a seam that reads before it appends (a
/// compare-and-append) must reach its append to be tested at all.
#[cfg(test)]
#[cfg_attr(all(feature = "core", not(feature = "store")), allow(dead_code))] // every consumer (spawn_store, run_store, progress_store, canary_store, mcpserver,
                                                                             // conductor) is store-gated, so this double is unused under core-only
pub struct SilentStore;

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
    fn last_position(&self, _stream: &str, _event_type: &str) -> Result<Option<Revision>, Error> {
        Ok(None)
    }
}

/// One call a [`ReadCountingStore`] forwarded, with how many events it handed back - the unit a
/// one-shot command's read cost is asserted in (spec 101). A subscription materializes its
/// events after the call returns, so it is recorded by where it starts, not by a count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CountedRead {
    Stream {
        stream: String,
        from: Revision,
        forward: bool,
        materialized: usize,
    },
    All {
        from: Position,
        forward: bool,
        materialized: usize,
    },
    SubscribeStream {
        stream: String,
        from: Revision,
    },
    SubscribeAll {
        from: Position,
    },
    LastPosition {
        stream: String,
        event_type: String,
    },
}

impl CountedRead {
    /// The events this call handed back to its caller.
    pub fn materialized(&self) -> usize {
        match self {
            CountedRead::Stream { materialized, .. } | CountedRead::All { materialized, .. } => {
                *materialized
            }
            CountedRead::SubscribeStream { .. }
            | CountedRead::SubscribeAll { .. }
            | CountedRead::LastPosition { .. } => 0,
        }
    }
}

/// THE COUNTING STORE DOUBLE (spec 101): a real store behind a decorator that records every read
/// it forwards and how many events each one materialized, so a test asserts a command's read
/// cost at the store seam instead of measuring resident memory. Appends pass through uncounted.
/// The one shared instance of this instrument: every criterion that asserts a read cost uses it,
/// and a new port method gains its forwarding and its [`CountedRead`] here.
pub struct ReadCountingStore<'a> {
    inner: &'a dyn EventStore,
    reads: std::sync::Mutex<Vec<CountedRead>>,
}

impl<'a> ReadCountingStore<'a> {
    /// Count the reads made through this decorator over `inner`.
    pub fn new(inner: &'a dyn EventStore) -> Self {
        ReadCountingStore {
            inner,
            reads: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Every read forwarded so far, in call order.
    pub fn reads(&self) -> Vec<CountedRead> {
        self.reads.lock().unwrap().clone()
    }

    /// The total events every read so far handed back.
    pub fn materialized(&self) -> usize {
        self.reads().iter().map(CountedRead::materialized).sum()
    }

    fn record(&self, read: CountedRead) {
        self.reads.lock().unwrap().push(read);
    }
}

impl EventStore for ReadCountingStore<'_> {
    fn append(
        &self,
        stream: &str,
        expected: ExpectedRevision,
        events: &[Event],
    ) -> Result<Appended, Error> {
        self.inner.append(stream, expected, events)
    }
    fn read_stream(
        &self,
        stream: &str,
        from: Revision,
        dir: Direction,
    ) -> Result<Vec<Event>, Error> {
        let events = self.inner.read_stream(stream, from, dir)?;
        self.record(CountedRead::Stream {
            stream: stream.to_string(),
            from,
            forward: matches!(dir, Direction::Forward),
            materialized: events.len(),
        });
        Ok(events)
    }
    fn read_all(
        &self,
        from: Position,
        dir: Direction,
        filter: &Filter,
    ) -> Result<Vec<Event>, Error> {
        let events = self.inner.read_all(from, dir, filter)?;
        self.record(CountedRead::All {
            from,
            forward: matches!(dir, Direction::Forward),
            materialized: events.len(),
        });
        Ok(events)
    }
    fn subscribe_all(&self, from: Position, filter: &Filter) -> Result<Subscription, Error> {
        self.record(CountedRead::SubscribeAll { from });
        self.inner.subscribe_all(from, filter)
    }
    fn subscribe_stream(&self, stream: &str, from: Revision) -> Result<Subscription, Error> {
        self.record(CountedRead::SubscribeStream {
            stream: stream.to_string(),
            from,
        });
        self.inner.subscribe_stream(stream, from)
    }
    fn last_position(&self, stream: &str, event_type: &str) -> Result<Option<Revision>, Error> {
        self.record(CountedRead::LastPosition {
            stream: stream.to_string(),
            event_type: event_type.to_string(),
        });
        self.inner.last_position(stream, event_type)
    }
}
