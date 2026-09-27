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

/// The four read and subscribe methods of an `EventStore` decorator that intercepts only
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
}
