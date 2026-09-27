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
