//! The run's REPLAY KEYS: the plain set every keyed lifecycle emit consults - an already-held
//! key appends nothing. It is seeded from the run's recorded events and extended with each key
//! this process emits.
//!
//! Perception is not in it (spec 107): neither a derived event's key nor a ledger entry's is ever
//! seeded, and the run's ingest sink asks no key set what to record - the log's latest generation
//! and the graph's current one decide that (`ingest::entry_of_batch`), so a replay key of
//! perception is unique to nothing and no reader treats it as such.

use std::collections::HashSet;
use std::sync::Mutex;

use crate::eventstore::Event;
use crate::ingest::META_REPLAY_KEY;
use crate::retention::PERCEPTION_TYPES;

/// The replay-key set of a run's keyed lifecycle emits.
pub(crate) struct ReplayKeys {
    keys: Mutex<HashSet<String>>,
}

impl ReplayKeys {
    /// A set seeded from `prior`, the run's recorded events: it holds the replay key of each
    /// one that carries a key and is not perception
    /// ([`PERCEPTION_TYPES`]). The type decides, never the key's spelling, so neither a derived
    /// event's key nor a ledger entry's is ever seeded.
    pub(crate) fn seeded(prior: &[Event]) -> Self {
        let keys = prior
            .iter()
            .filter(|e| !PERCEPTION_TYPES.contains(&e.type_.as_str()))
            .filter_map(|e| e.meta.get(META_REPLAY_KEY).cloned())
            .collect();
        ReplayKeys {
            keys: Mutex::new(keys),
        }
    }

    /// Record `key`: `true` when it is new work, `false` when the set already held it (a replay).
    pub(crate) fn insert(&self, key: &str) -> bool {
        self.keys.lock().unwrap().insert(key.to_string())
    }

    /// Whether the set holds `key`.
    pub(crate) fn contains(&self, key: &str) -> bool {
        self.keys.lock().unwrap().contains(key)
    }
}

#[cfg(test)]
mod seed_tests {
    use super::ReplayKeys;
    use crate::eventstore::Event;
    use crate::ingest::DERIVED_INDEX_TYPES;
    use crate::ingest::META_REPLAY_KEY;
    use crate::test_support::generation_ingested;

    /// READERS SKIP PERCEPTION, the seeded key set: of a run's keyed events, a lifecycle event's
    /// key is seeded, and neither a ledger entry's nor any derived event's is, whatever its
    /// spelling; an event carrying no key seeds nothing.
    #[test]
    fn the_seed_holds_each_lifecycle_key_and_no_perception_key() {
        let keyed =
            |type_: &str, key: &str| Event::new(type_, vec![]).with_meta(META_REPLAY_KEY, key);
        let entry = generation_ingested("gc", "src/a.rs", "h1", "b1", false).event(2);
        assert_eq!(
            entry.meta.get(META_REPLAY_KEY).map(String::as_str),
            Some("gc/src/a.rs@h1#2")
        );
        let derived_keys = ["derived#0", "derived#1", "derived#2", "derived#3"];
        let mut prior = vec![
            keyed("UnitStarted", "unit:u1:started"),
            entry,
            Event::new("UnitStarted", vec![]),
        ];
        prior.extend(
            DERIVED_INDEX_TYPES
                .iter()
                .zip(derived_keys)
                .map(|(type_, key)| keyed(type_, key)),
        );
        prior.push(keyed("GateVerdict", "gate:u1:fmt"));

        let set = ReplayKeys::seeded(&prior);
        assert!(set.contains("unit:u1:started"));
        assert!(set.contains("gate:u1:fmt"));
        assert!(!set.contains("gc/src/a.rs@h1#2"), "a ledger entry's key");
        for key in derived_keys {
            assert!(!set.contains(key), "a derived event's key: {key}");
        }
        assert!(
            set.insert("gc/src/a.rs@h1#2"),
            "an unseeded key is new work"
        );
        assert!(!set.insert("unit:u1:started"), "a seeded key is a replay");
    }

    #[test]
    fn a_seed_of_no_event_holds_no_key() {
        let set = ReplayKeys::seeded(&[]);
        assert!(!set.contains("unit:u1:started"));
        assert!(set.insert("unit:u1:started"));
    }

    #[test]
    fn a_key_is_new_once_and_held_after() {
        let set =
            ReplayKeys::seeded(&[Event::new("T", vec![]).with_meta(META_REPLAY_KEY, "seeded")]);
        assert!(set.contains("seeded"));
        assert!(!set.insert("seeded"), "a seeded key is a replay");
        assert!(!set.contains("fresh"));
        assert!(set.insert("fresh"), "an unseen key is new work");
        assert!(!set.insert("fresh"), "and a replay after");
        assert!(set.contains("fresh"));
    }
}
