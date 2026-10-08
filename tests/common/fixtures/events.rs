//! Event fixtures.

use rigger::eventstore::{
    Appended, Direction, Error, Event, EventBatchSink, EventStore, ExpectedRevision, Filter,
    GroupHead, Position, Revision, Subscription, TypeSelection,
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

/// Each event's type, in the order given.
pub fn types_of(events: &[Event]) -> Vec<&str> {
    events.iter().map(|e| e.type_.as_str()).collect()
}

/// How many of `events` are of type `type_`.
pub fn count_of_type(events: &[Event], type_: &str) -> usize {
    events.iter().filter(|e| e.type_ == type_).count()
}

/// The JSON payload of every event of type `type_` in `events`, in order.
pub fn payloads_of_type(events: &[Event], type_: &str) -> Vec<serde_json::Value> {
    events
        .iter()
        .filter(|e| e.type_ == type_)
        .map(|e| serde_json::from_slice(&e.data).unwrap())
        .collect()
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
/// decorator's `impl EventStore` block. `(stream)` expands every one of them but the group lookup,
/// for a decorator that also intercepts `latest_in_group`.
#[macro_export]
macro_rules! delegate_event_store_reads {
    () => {
        $crate::delegate_event_store_reads!(stream);
        fn latest_in_group(
            &self,
            stream: &str,
            group: &str,
        ) -> Result<Option<rigger::eventstore::GroupHead>, rigger::eventstore::Error> {
            self.inner.latest_in_group(stream, group)
        }
    };
    (stream) => {
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
        fn read_stream_typed(
            &self,
            stream: &str,
            from: rigger::eventstore::Revision,
            selection: rigger::eventstore::TypeSelection,
        ) -> Result<Vec<rigger::eventstore::Event>, rigger::eventstore::Error> {
            self.inner.read_stream_typed(stream, from, selection)
        }
        fn read_stream_positions(
            &self,
            stream: &str,
            batch: usize,
            sink: &mut dyn FnMut(
                &[rigger::eventstore::Position],
            ) -> Result<(), rigger::eventstore::Error>,
        ) -> Result<(), rigger::eventstore::Error> {
            self.inner.read_stream_positions(stream, batch, sink)
        }
        fn read_stream_batched(
            &self,
            stream: &str,
            from: rigger::eventstore::Revision,
            batch: usize,
            sink: &mut rigger::eventstore::EventBatchSink,
        ) -> Result<(), rigger::eventstore::Error> {
            self.inner.read_stream_batched(stream, from, batch, sink)
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
    fn read_stream_typed(
        &self,
        _stream: &str,
        _from: Revision,
        _selection: TypeSelection,
    ) -> Result<Vec<Event>, Error> {
        Ok(Vec::new())
    }
    fn read_stream_positions(
        &self,
        _stream: &str,
        _batch: usize,
        _sink: &mut dyn FnMut(&[Position]) -> Result<(), Error>,
    ) -> Result<(), Error> {
        Ok(())
    }
    fn read_stream_batched(
        &self,
        _stream: &str,
        _from: Revision,
        _batch: usize,
        _sink: &mut EventBatchSink,
    ) -> Result<(), Error> {
        Ok(())
    }
    fn latest_in_group(&self, _stream: &str, _group: &str) -> Result<Option<GroupHead>, Error> {
        Ok(None)
    }
}

/// The port methods a double REFUSES, each panicking with `$why` and what the caller tried -
/// expanded inside that double's `impl EventStore` block, naming the methods it does not answer.
macro_rules! refuse_event_store_calls {
    ($why:literal: $($method:ident),+ $(,)?) => {
        $(refuse_event_store_calls!(@ $why $method);)+
    };
    (@ $why:literal append) => {
        fn append(&self, _: &str, _: ExpectedRevision, _: &[Event]) -> Result<Appended, Error> {
            panic!(concat!($why, ": nothing appends"))
        }
    };
    (@ $why:literal read_stream) => {
        fn read_stream(&self, _: &str, _: Revision, _: Direction) -> Result<Vec<Event>, Error> {
            panic!(concat!($why, ": nothing reads the stream"))
        }
    };
    (@ $why:literal read_all) => {
        fn read_all(&self, _: Position, _: Direction, _: &Filter) -> Result<Vec<Event>, Error> {
            panic!(concat!($why, ": nothing reads the log"))
        }
    };
    (@ $why:literal subscribe_all) => {
        fn subscribe_all(&self, _: Position, _: &Filter) -> Result<Subscription, Error> {
            panic!(concat!($why, ": nothing subscribes"))
        }
    };
    (@ $why:literal subscribe_stream) => {
        fn subscribe_stream(&self, _: &str, _: Revision) -> Result<Subscription, Error> {
            panic!(concat!($why, ": nothing subscribes"))
        }
    };
    (@ $why:literal last_position) => {
        fn last_position(&self, _: &str, _: &str) -> Result<Option<Revision>, Error> {
            panic!(concat!($why, ": nothing looks up a boundary"))
        }
    };
    (@ $why:literal read_stream_typed) => {
        fn read_stream_typed(
            &self,
            _: &str,
            _: Revision,
            _: TypeSelection,
        ) -> Result<Vec<Event>, Error> {
            panic!(concat!($why, ": nothing reads by type"))
        }
    };
    (@ $why:literal read_stream_positions) => {
        fn read_stream_positions(
            &self,
            _: &str,
            _: usize,
            _: &mut dyn FnMut(&[Position]) -> Result<(), Error>,
        ) -> Result<(), Error> {
            panic!(concat!($why, ": nothing reads positions"))
        }
    };
    (@ $why:literal read_stream_batched) => {
        fn read_stream_batched(
            &self,
            _: &str,
            _: Revision,
            _: usize,
            _: &mut EventBatchSink,
        ) -> Result<(), Error> {
            panic!(concat!($why, ": nothing reads the stream in batches"))
        }
    };
    (@ $why:literal latest_in_group) => {
        fn latest_in_group(&self, _: &str, _: &str) -> Result<Option<GroupHead>, Error> {
            panic!(concat!($why, ": nothing looks up a group"))
        }
    };
}

/// A store whose ONLY reachable port method is the group lookup (spec 101): it answers every group
/// with one fixed answer - a newest member, no member, or a backend error - and records each
/// `(stream, group)` it was asked. Every other method panics, so a caller that reads the stream,
/// subscribes or appends where it should only have asked the lookup fails.
pub struct GroupLookupOnly {
    answer: Result<Option<GroupHead>, String>,
    asked: std::sync::Mutex<Vec<(String, String)>>,
}

impl GroupLookupOnly {
    /// A store answering every group with `answer`: `Ok` the newest member (or none), `Err` a
    /// backend error carrying that message.
    pub fn new(answer: Result<Option<GroupHead>, String>) -> Self {
        GroupLookupOnly {
            answer,
            asked: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Every `(stream, group)` the lookup was asked, in order.
    pub fn asked(&self) -> Vec<(String, String)> {
        self.asked.lock().unwrap().clone()
    }
}

impl EventStore for GroupLookupOnly {
    refuse_event_store_calls!(
        "only the group lookup is reachable":
        append,
        read_stream,
        read_all,
        subscribe_all,
        subscribe_stream,
        last_position,
        read_stream_typed,
        read_stream_positions,
        read_stream_batched,
    );
    fn latest_in_group(&self, stream: &str, group: &str) -> Result<Option<GroupHead>, Error> {
        self.asked
            .lock()
            .unwrap()
            .push((stream.to_string(), group.to_string()));
        self.answer.clone().map_err(Error::Backend)
    }
}

/// A read-only store over ONE hand-built stream, for a use case tested where no backend is in
/// reach: its events are stamped in the order given as the sqlite store stamps an append to an
/// empty log, per-stream revisions from 0 and log positions from 1, and only the boundary lookup
/// and the typed read answer, as the port words them. Every other method panics, and another
/// stream holds nothing.
pub struct HandBuiltLog {
    stream: String,
    events: Vec<Event>,
}

impl HandBuiltLog {
    /// `stream` holding `events` in this order.
    pub fn new(stream: &str, mut events: Vec<Event>) -> Self {
        for (i, e) in events.iter_mut().enumerate() {
            e.stream = stream.to_string();
            e.position = (i + 1) as u64;
            e.revision = i as Revision;
        }
        HandBuiltLog {
            stream: stream.to_string(),
            events,
        }
    }

    /// The events of `stream`: this log's own, or none for any other stream.
    fn of(&self, stream: &str) -> &[Event] {
        if stream == self.stream {
            &self.events
        } else {
            &[]
        }
    }
}

impl EventStore for HandBuiltLog {
    refuse_event_store_calls!(
        "only the boundary lookup and the typed read are reachable":
        append,
        read_stream,
        read_all,
        subscribe_all,
        subscribe_stream,
        read_stream_positions,
        read_stream_batched,
        latest_in_group,
    );
    fn last_position(&self, stream: &str, event_type: &str) -> Result<Option<Revision>, Error> {
        Ok(self
            .of(stream)
            .iter()
            .rev()
            .find(|e| e.type_ == event_type)
            .map(|e| e.revision))
    }
    fn read_stream_typed(
        &self,
        stream: &str,
        from: Revision,
        selection: TypeSelection,
    ) -> Result<Vec<Event>, Error> {
        Ok(self
            .of(stream)
            .iter()
            .filter(|e| e.revision >= from)
            .filter(|e| match selection {
                TypeSelection::Only(types) => types.contains(&e.type_.as_str()),
                TypeSelection::Except(types) => !types.contains(&e.type_.as_str()),
            })
            .cloned()
            .collect())
    }
}

/// An `EventStore` decorator that forwards every call to `inner` unchanged except `append`, which
/// refuses (a real `Backend` error, indistinguishable from a genuine backend fault) any batch
/// holding an event that carries a metadata VALUE containing `needle`. It matches metadata, never
/// the payload, because what tells one write from another often lives only there: a
/// `GateVerdict`'s pre-merge-vs-post-merge identity is its replay key (`gate:` vs
/// `postmerge-gate:`), and a derived index batch's file is its group and replay key.
pub struct FailAppendMetaContaining<'a> {
    pub inner: &'a dyn EventStore,
    pub needle: &'static str,
}

impl EventStore for FailAppendMetaContaining<'_> {
    fn append(
        &self,
        stream: &str,
        expected: ExpectedRevision,
        events: &[Event],
    ) -> Result<Appended, Error> {
        if events
            .iter()
            .any(|e| e.meta.values().any(|v| v.contains(self.needle)))
        {
            return Err(Error::Backend(format!(
                "simulated store failure appending an event whose metadata contains {:?}",
                self.needle
            )));
        }
        self.inner.append(stream, expected, events)
    }
    crate::delegate_event_store_reads!();
}

/// One call a [`ReadCountingStore`] forwarded, with how many events it handed back - the unit a
/// one-shot command's read cost is asserted in (spec 101). A call is recorded before it is
/// forwarded, so one that fails stays recorded, having handed back nothing; a subscription counts
/// each event it delivers after the call returns.
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
        delivered: usize,
    },
    SubscribeAll {
        from: Position,
        delivered: usize,
    },
    LastPosition {
        stream: String,
        event_type: String,
    },
    /// A positions read: `handed` is how many positions it handed the caller - never an event,
    /// so it materializes nothing.
    StreamPositions {
        stream: String,
        handed: usize,
    },
    /// A batched stream read: `batches` is how many events each batch it handed the caller
    /// held, in order - so a test pins both the total and the bound on any one batch.
    StreamBatched {
        stream: String,
        from: Revision,
        batches: Vec<usize>,
    },
    /// A group lookup: it hands back no event, only the newest member's position, type and
    /// metadata.
    LatestInGroup {
        stream: String,
        group: String,
    },
    /// A typed read: `only` is whether the selection named the types it hands back (`Only`)
    /// or the ones it refuses (`Except`), and `types` are those names in the order given.
    Typed {
        stream: String,
        from: Revision,
        only: bool,
        types: Vec<String>,
        materialized: usize,
    },
}

impl CountedRead {
    /// The events this call handed back to its caller so far.
    pub fn materialized(&self) -> usize {
        match self {
            CountedRead::Stream { materialized, .. }
            | CountedRead::All { materialized, .. }
            | CountedRead::Typed { materialized, .. } => *materialized,
            CountedRead::SubscribeStream { delivered, .. }
            | CountedRead::SubscribeAll { delivered, .. } => *delivered,
            CountedRead::StreamBatched { batches, .. } => batches.iter().sum(),
            CountedRead::LastPosition { .. }
            | CountedRead::StreamPositions { .. }
            | CountedRead::LatestInGroup { .. } => 0,
        }
    }

    /// This call with its count cleared: what was asked, not what came back.
    pub fn uncounted(&self) -> CountedRead {
        let mut read = self.clone();
        match &mut read {
            CountedRead::Stream { materialized, .. }
            | CountedRead::All { materialized, .. }
            | CountedRead::Typed { materialized, .. } => *materialized = 0,
            CountedRead::SubscribeStream { delivered, .. }
            | CountedRead::SubscribeAll { delivered, .. } => *delivered = 0,
            CountedRead::LastPosition { .. } | CountedRead::LatestInGroup { .. } => {}
            CountedRead::StreamPositions { handed, .. } => *handed = 0,
            CountedRead::StreamBatched { batches, .. } => batches.clear(),
        }
        read
    }

    /// Add `n` handed-back events - or positions, for a positions read, or one batch of `n`, for a
    /// batched read - to this call's count; a lookup hands back none.
    fn add(&mut self, n: usize) {
        match self {
            CountedRead::Stream { materialized, .. }
            | CountedRead::All { materialized, .. }
            | CountedRead::Typed { materialized, .. } => *materialized += n,
            CountedRead::SubscribeStream { delivered, .. }
            | CountedRead::SubscribeAll { delivered, .. } => *delivered += n,
            CountedRead::StreamPositions { handed, .. } => *handed += n,
            CountedRead::StreamBatched { batches, .. } => batches.push(n),
            CountedRead::LastPosition { .. } | CountedRead::LatestInGroup { .. } => {}
        }
    }
}

/// The shared call log a [`ReadCountingStore`] and the subscriptions it hands back count into.
type CallLog = std::sync::Arc<std::sync::Mutex<Vec<CountedRead>>>;

/// THE COUNTING STORE DOUBLE (spec 101): a real store behind a decorator that records every read
/// it forwards and how many events each one materialized, so a test asserts a command's read
/// cost at the store seam instead of measuring resident memory. Appends pass through uncounted.
/// The one shared instance of this instrument: every criterion that asserts a read cost uses it,
/// and a new port method gains its forwarding and its [`CountedRead`] here.
pub struct ReadCountingStore<'a> {
    inner: &'a dyn EventStore,
    reads: CallLog,
    /// A write waiting to land in `inner` when a given call returns ([`Self::interleaving`]).
    interleaved: std::sync::Mutex<Option<(usize, String, Vec<Event>)>>,
}

impl<'a> ReadCountingStore<'a> {
    /// Count the reads made through this decorator over `inner`.
    pub fn new(inner: &'a dyn EventStore) -> Self {
        ReadCountingStore {
            inner,
            reads: CallLog::default(),
            interleaved: std::sync::Mutex::default(),
        }
    }

    /// This double with a concurrent writer: `events` land on `stream` of the inner store the
    /// moment the call at index `after` of the call log (0-based, in call order; a boundary lookup
    /// counts) returns, so a test places an append exactly between two calls of one command.
    pub fn interleaving(self, after: usize, stream: &str, events: Vec<Event>) -> Self {
        *self.interleaved.lock().unwrap() = Some((after, stream.to_string(), events));
        self
    }

    /// Every read forwarded so far, in call order.
    pub fn reads(&self) -> Vec<CountedRead> {
        self.reads.lock().unwrap().clone()
    }

    /// The total events every read so far handed back.
    pub fn materialized(&self) -> usize {
        self.reads().iter().map(CountedRead::materialized).sum()
    }

    /// Record `read` before it is forwarded, answering its index in the call log.
    fn record(&self, read: CountedRead) -> usize {
        let mut reads = self.reads.lock().unwrap();
        reads.push(read);
        reads.len() - 1
    }

    /// Count the `events` call `at` handed back, then hand them on.
    fn handed_back(&self, at: usize, events: Vec<Event>) -> Vec<Event> {
        self.reads.lock().unwrap()[at].add(events.len());
        self.land_interleaved(at);
        events
    }

    /// Append the interleaved write when it waits on call `at`, once.
    fn land_interleaved(&self, at: usize) {
        let mut pending = self.interleaved.lock().unwrap();
        if pending.as_ref().is_some_and(|(after, ..)| *after == at) {
            let (_, stream, events) = pending.take().unwrap();
            self.inner
                .append(&stream, ExpectedRevision::Any, &events)
                .expect("the interleaved write appends");
        }
    }

    /// `sub` relayed so each event it delivers is counted into call `at` before it is handed on.
    fn counted(&self, at: usize, sub: Subscription) -> Subscription {
        let reads = std::sync::Arc::clone(&self.reads);
        sub.map(move |e| {
            reads.lock().unwrap()[at].add(1);
            e
        })
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
        let at = self.record(CountedRead::Stream {
            stream: stream.to_string(),
            from,
            forward: matches!(dir, Direction::Forward),
            materialized: 0,
        });
        let events = self.inner.read_stream(stream, from, dir)?;
        Ok(self.handed_back(at, events))
    }
    fn read_all(
        &self,
        from: Position,
        dir: Direction,
        filter: &Filter,
    ) -> Result<Vec<Event>, Error> {
        let at = self.record(CountedRead::All {
            from,
            forward: matches!(dir, Direction::Forward),
            materialized: 0,
        });
        let events = self.inner.read_all(from, dir, filter)?;
        Ok(self.handed_back(at, events))
    }
    fn subscribe_all(&self, from: Position, filter: &Filter) -> Result<Subscription, Error> {
        let at = self.record(CountedRead::SubscribeAll { from, delivered: 0 });
        let sub = self.inner.subscribe_all(from, filter)?;
        Ok(self.counted(at, sub))
    }
    fn subscribe_stream(&self, stream: &str, from: Revision) -> Result<Subscription, Error> {
        let at = self.record(CountedRead::SubscribeStream {
            stream: stream.to_string(),
            from,
            delivered: 0,
        });
        let sub = self.inner.subscribe_stream(stream, from)?;
        Ok(self.counted(at, sub))
    }
    fn last_position(&self, stream: &str, event_type: &str) -> Result<Option<Revision>, Error> {
        let at = self.record(CountedRead::LastPosition {
            stream: stream.to_string(),
            event_type: event_type.to_string(),
        });
        let boundary = self.inner.last_position(stream, event_type);
        self.land_interleaved(at);
        boundary
    }
    fn read_stream_typed(
        &self,
        stream: &str,
        from: Revision,
        selection: TypeSelection,
    ) -> Result<Vec<Event>, Error> {
        let (only, types) = match selection {
            TypeSelection::Only(types) => (true, types),
            TypeSelection::Except(types) => (false, types),
        };
        let at = self.record(CountedRead::Typed {
            stream: stream.to_string(),
            from,
            only,
            types: types.iter().map(|t| t.to_string()).collect(),
            materialized: 0,
        });
        let events = self.inner.read_stream_typed(stream, from, selection)?;
        Ok(self.handed_back(at, events))
    }
    fn read_stream_positions(
        &self,
        stream: &str,
        batch: usize,
        sink: &mut dyn FnMut(&[Position]) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let at = self.record(CountedRead::StreamPositions {
            stream: stream.to_string(),
            handed: 0,
        });
        self.inner
            .read_stream_positions(stream, batch, &mut |positions| {
                self.reads.lock().unwrap()[at].add(positions.len());
                sink(positions)
            })?;
        self.land_interleaved(at);
        Ok(())
    }
    fn read_stream_batched(
        &self,
        stream: &str,
        from: Revision,
        batch: usize,
        sink: &mut EventBatchSink,
    ) -> Result<(), Error> {
        let at = self.record(CountedRead::StreamBatched {
            stream: stream.to_string(),
            from,
            batches: Vec::new(),
        });
        self.inner
            .read_stream_batched(stream, from, batch, &mut |events, head| {
                self.reads.lock().unwrap()[at].add(events.len());
                sink(events, head)
            })?;
        self.land_interleaved(at);
        Ok(())
    }
    fn latest_in_group(&self, stream: &str, group: &str) -> Result<Option<GroupHead>, Error> {
        let at = self.record(CountedRead::LatestInGroup {
            stream: stream.to_string(),
            group: group.to_string(),
        });
        let head = self.inner.latest_in_group(stream, group);
        self.land_interleaved(at);
        head
    }
}

/// The derived index types a one-shot fixture floods the log with - the four
/// `ingest::DERIVED_INDEX_TYPES`, spelled out because this fixture compiles into crates that do
/// not all see the `ingest` module (a test asserting on them compares against that constant).
pub const ONE_SHOT_DERIVED_TYPES: [&str; 4] = [
    "CodeEntityExtracted",
    "EdgeInferred",
    "DocConceptExtracted",
    "DocLinkExtracted",
];

/// The perception types a read of the run refuses at the store - the five
/// `retention::PERCEPTION_TYPES`, spelled out for the reason [`ONE_SHOT_DERIVED_TYPES`] is (the
/// tests of `run::read` hold this list to that constant).
pub const ONE_SHOT_PERCEPTION_TYPES: [&str; 5] = [
    "CodeEntityExtracted",
    "EdgeInferred",
    "DocConceptExtracted",
    "DocLinkExtracted",
    "GenerationIngested",
];

/// What a one-shot command may cost over [`seed_one_shot_fixture`]'s log (spec 101): the current
/// run's own non-perception events and the carried-over knowledge of every run, and nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OneShotFixture {
    /// The per-stream revision of the current run's `RunStarted`.
    pub boundary: Revision,
    /// The current run's own events a one-shot read hands back: its `RunStarted`, its decision,
    /// its finding and its two notes (never the derived events appended during it).
    pub run_events: usize,
    /// The `DecisionMade`, `LessonLearned` and `ReviewFinding` events of every run.
    pub carry_over: usize,
}

impl OneShotFixture {
    /// The events one read of the run materializes: the run's own plus the carry-over.
    pub fn cost(&self) -> usize {
        self.run_events + self.carry_over
    }

    /// The three calls ONE read of the run on `stream` makes, as the counting double records
    /// them: the boundary lookup, the carried-over knowledge by type over the whole stream, and
    /// the run slice from the boundary with the perception types refused.
    pub fn read(&self, stream: &str) -> Vec<CountedRead> {
        let carry = ["DecisionMade", "LessonLearned", "ReviewFinding"];
        let names = |types: &[&str]| types.iter().map(|t| t.to_string()).collect::<Vec<_>>();
        vec![
            CountedRead::LastPosition {
                stream: stream.to_string(),
                event_type: "RunStarted".to_string(),
            },
            CountedRead::Typed {
                stream: stream.to_string(),
                from: 0,
                only: true,
                types: names(&carry),
                materialized: self.carry_over,
            },
            CountedRead::Typed {
                stream: stream.to_string(),
                from: self.boundary,
                only: false,
                types: names(&ONE_SHOT_PERCEPTION_TYPES),
                materialized: self.run_events,
            },
        ]
    }

    /// `times` reads of the run on `stream`, back to back.
    pub fn reads(&self, stream: &str, times: usize) -> Vec<CountedRead> {
        (0..times).flat_map(|_| self.read(stream)).collect()
    }
}

/// A `RunStarted` for run `run` over `criteria`, stamped with its run id as the conductor mints it.
pub fn run_started(run: &str, criteria: &[&str]) -> Event {
    Event::new(
        "RunStarted",
        serde_json::to_vec(&serde_json::json!({"run": run, "criteria": criteria})).unwrap(),
    )
    .with_meta("run_id", run)
}

/// `n` derived index events, cycling the four [`ONE_SHOT_DERIVED_TYPES`].
fn derived_events(n: usize) -> Vec<Event> {
    (0..n)
        .map(|i| {
            ev(
                ONE_SHOT_DERIVED_TYPES[i % 4],
                r#"{"from":"a","rel":"CALLS","to":"b"}"#,
            )
        })
        .collect()
}

/// THE ONE-SHOT FIXTURE (spec 101): `stream` holds two superseded runs with 200,000 derived index
/// events before the current run's boundary, then the current run (started over `criteria`) with
/// derived events of its own appended during it. Each prior run left a decision, a lesson or a
/// finding the current run carries over, and a note of its own no one-shot read carries; the
/// current run holds a decision and a finding of its own plus two notes.
pub fn seed_one_shot_fixture(
    store: &dyn EventStore,
    stream: &str,
    criteria: &[&str],
) -> OneShotFixture {
    let append = |events: &[Event]| {
        store
            .append(stream, ExpectedRevision::Any, events)
            .expect("the one-shot fixture appends");
    };
    let decision = |id: &str, file: &str| {
        ev(
            "DecisionMade",
            &format!(r#"{{"id":"{id}","summary":"chose {id}","governs":["{file}"]}}"#),
        )
    };
    append(&[
        run_started("run-a", &["a prior campaign"]),
        ev("UnitStarted", r#"{"id":"ua","unit":"ua","agent":"impl"}"#),
        decision("d-a", "a.rs"),
        ev(
            "LessonLearned",
            r#"{"id":"l-a","summary":"learned a","about":["a.rs"]}"#,
        ),
        ev("RunNote", "{}"),
    ]);
    for _ in 0..10 {
        append(&derived_events(10_000));
    }
    append(&[
        run_started("run-b", &["another prior campaign"]),
        ev("UnitStarted", r#"{"id":"ub","unit":"ub","agent":"impl"}"#),
        ev(
            "ReviewFinding",
            r#"{"id":"f-b","by":"lens","summary":"found b","about":["b.rs"]}"#,
        ),
        ev("RunNote", "{}"),
    ]);
    for _ in 0..10 {
        append(&derived_events(10_000));
    }
    append(&[run_started("run-c", criteria), ev("RunNote", "{}")]);
    let boundary = store
        .last_position(stream, "RunStarted")
        .expect("the fixture's boundary reads")
        .expect("the fixture started a run");
    append(&derived_events(50));
    append(&[
        decision("d-c", "c.rs"),
        ev(
            "ReviewFinding",
            r#"{"id":"f-c","by":"lens","summary":"found c","about":["c.rs"]}"#,
        ),
        ev("RunNote", "{}"),
    ]);
    append(&derived_events(50));
    OneShotFixture {
        boundary,
        run_events: 5,
        carry_over: 5,
    }
}

/// THE ONE-SHOT PROGRESS FIXTURE (spec 101): the progress store beside [`seed_one_shot_fixture`]'s
/// log - two reports from each superseded run (`run-a`, `run-b`) and `current` from the current
/// run `run-c`, each on its run's own progress stream (`progress/<run>`, spelled out because this
/// fixture compiles into crates that do not all see the `progress` module) and stamped with its
/// run, as the progress writer records them - so a read of the current run's progress costs
/// exactly `current` reports.
pub fn seed_one_shot_progress(store: &dyn EventStore, current: usize) {
    let report = |run: &str, n: usize| {
        for i in 0..n {
            let body =
                serde_json::json!({"id": "u/implementer#0", "activity": format!("{run} step {i}")});
            store
                .append(
                    &format!("progress/{run}"),
                    ExpectedRevision::Any,
                    &[
                        Event::new("AgentProgress", serde_json::to_vec(&body).unwrap())
                            .with_meta("run_id", run),
                    ],
                )
                .expect("the one-shot progress fixture appends");
        }
    };
    report("run-a", 2);
    report("run-b", 2);
    report("run-c", current);
}
