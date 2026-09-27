//! Event fixtures.

use rigger::eventstore::Event;

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
