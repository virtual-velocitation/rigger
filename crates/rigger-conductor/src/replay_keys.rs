//! The run's REPLAY KEYS and, for the derived index, the generation each batch identity is tracked
//! at: the one owner of both sets and of the order they are locked in.
//!
//! The keys are a set every keyed emit consults - an already-held key appends nothing. The
//! generations map (symbols lane only) names, per batch identity (`<prefix>/<file>`), the content
//! generation this process currently tracks for it and the keys THAT generation put in the set, so
//! a fresh generation retires its predecessor's keys and a revert can never be shadowed by a key
//! an earlier generation left behind.
//!
//! Lock order is a property of this type, not a comment: [`ReplayKeys::insert`] and
//! [`ReplayKeys::contains`] lock the keys alone, and the only methods that lock both -
//! [`ReplayKeys::install`] and [`ReplayKeys::forget`] - take the generations first and the keys
//! second, so no two callers can ever hold them in opposite orders.

use std::collections::HashSet;
use std::sync::Mutex;

use crate::eventstore::Event;
use crate::ingest::META_REPLAY_KEY;
use crate::retention::PERCEPTION_TYPES;
#[cfg(feature = "symbols")]
use std::collections::{hash_map::Entry, HashMap};

/// The replay-key set and the per-identity generations it holds derived keys for.
pub(crate) struct ReplayKeys {
    #[cfg(feature = "symbols")]
    generations: Mutex<Generations>,
    keys: Mutex<HashSet<String>>,
}

/// The tracked slot of every identity met, and the last epoch minted for one.
#[cfg(feature = "symbols")]
#[derive(Default)]
struct Generations {
    slots: HashMap<String, Slot>,
    minted: u64,
}

/// One identity's tracked generation, the keys that generation contributed to the set, and the
/// EPOCH of the install that created it. A switch of generation drops the old slot and creates a
/// new one, so every slot's epoch is minted afresh: an install that tracked a generation which has
/// since moved away and come back is told apart from the install that brought it back.
#[cfg(feature = "symbols")]
struct Slot {
    generation: String,
    keys: HashSet<String>,
    epoch: u64,
}

/// What one [`ReplayKeys::install`] tracked, handed back to [`ReplayKeys::forget`]: the identity
/// and the epoch its slot carried, or nothing for a batch naming no identity.
#[cfg(feature = "symbols")]
pub(crate) struct Ticket(Option<(String, u64)>);

/// The next epoch, never one minted before.
#[cfg(feature = "symbols")]
fn mint(minted: &mut u64) -> u64 {
    *minted += 1;
    *minted
}

impl ReplayKeys {
    /// A set seeded from `prior`, the run's recorded events, tracking no generation: it holds
    /// the replay key of each one that carries a key and is not perception
    /// ([`PERCEPTION_TYPES`]). The type decides, never the key's spelling, so neither a derived
    /// event's key nor a ledger entry's is ever seeded.
    pub(crate) fn seeded(prior: &[Event]) -> Self {
        let keys = prior
            .iter()
            .filter(|e| !PERCEPTION_TYPES.contains(&e.type_.as_str()))
            .filter_map(|e| e.meta.get(META_REPLAY_KEY).cloned())
            .collect();
        ReplayKeys {
            #[cfg(feature = "symbols")]
            generations: Mutex::default(),
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

/// The identity and generation a keyed batch names, read from its first key: every key of one
/// batch shares both. `None` for an empty batch or one whose keys are not the per-file key shape.
#[cfg(feature = "symbols")]
fn identity_generation<'k>(keyed: &'k [(String, &Event)]) -> Option<(&'k str, &'k str)> {
    keyed
        .first()
        .and_then(|(key, _)| crate::ingest::derived_key_parts(key))
}

#[cfg(feature = "symbols")]
impl ReplayKeys {
    /// Decide what of the keyed batch `keyed` appends, and record it as appended: the survivors,
    /// in batch order, each with its key, and the [`Ticket`] a failed append hands to
    /// [`forget`](ReplayKeys::forget).
    ///
    /// The FIRST time an identity is met, `first_sight` answers whether the batch is that
    /// identity's latest recorded generation (spec 101). It is asked under the generations lock,
    /// so the unseen check, the answer and the slot it installs are one step: a concurrent call
    /// meeting the same identity waits, then finds the slot. An unanswered lookup is this call's
    /// error and installs nothing. A batch that is its identity's latest recorded generation
    /// installs its keys, so none of it survives.
    ///
    /// A batch naming a generation other than the one its identity's slot tracks retires that slot
    /// and its keys first (spec 86 criterion 3), so the fresh generation is never shadowed by
    /// a stale key that happens to hash identically. Then each event `rebuild` answers survives
    /// unless the set already holds its key; `rebuild` answering `None` skips the event without
    /// recording its key. A batch naming no identity asks nothing and tracks no generation: it is
    /// the plain dedup alone.
    pub(crate) fn install<E>(
        &self,
        keyed: &[(String, &Event)],
        first_sight: impl FnOnce() -> Result<bool, E>,
        mut rebuild: impl FnMut(&str, &Event) -> Option<Event>,
    ) -> Result<(Vec<(String, Event)>, Ticket), E> {
        let named = identity_generation(keyed);
        let mut generations = self.generations.lock().unwrap();
        let Generations { slots, minted } = &mut *generations;
        let recorded = match named {
            Some((identity, _)) if !slots.contains_key(identity) => first_sight()?,
            _ => false,
        };
        let mut keys = self.keys.lock().unwrap();
        let mut slot = named.map(|(identity, generation)| {
            if let Entry::Occupied(held) = slots.entry(identity.to_string()) {
                if held.get().generation != generation {
                    for stale in held.remove().keys {
                        keys.remove(&stale);
                    }
                }
            }
            let slot = slots.entry(identity.to_string()).or_insert_with(|| Slot {
                generation: generation.to_string(),
                keys: HashSet::new(),
                epoch: mint(minted),
            });
            if recorded {
                for (key, _) in keyed {
                    keys.insert(key.clone());
                    slot.keys.insert(key.clone());
                }
            }
            slot
        });
        let mut survivors = Vec::new();
        for (key, ev) in keyed {
            let Some(rebuilt) = rebuild(key, ev) else {
                continue;
            };
            if keys.insert(key.clone()) {
                if let Some(slot) = slot.as_mut() {
                    slot.keys.insert(key.clone());
                }
                survivors.push((key.clone(), rebuilt));
            }
        }
        let ticket = Ticket(
            named
                .zip(slot)
                .map(|((identity, _), slot)| (identity.to_string(), slot.epoch)),
        );
        Ok((survivors, ticket))
    }

    /// Forget what the [`install`](ReplayKeys::install) that handed out `ticket` recorded, after
    /// its append failed and recorded nothing. `kept` is that install's survivors' keys.
    ///
    /// A batch naming no identity tracked no slot: its `kept` keys leave the set. Otherwise the
    /// install is recognised by its slot's epoch, not its generation string. While the identity's
    /// slot still carries the ticket's epoch, the `kept` keys leave the set and the slot, and a
    /// slot left holding no keys is removed - so the next sight asks the store afresh and follows
    /// what the store holds. Once the slot carries another epoch (it switched generation, perhaps
    /// back to this very one, or was removed and re-created), every key the identity holds belongs
    /// to a later install, and this forget touches nothing for it.
    ///
    /// THE IN-FLIGHT WINDOW: both locks are released across the append, so between the install and
    /// this forget a concurrent call emitting the SAME identity and generation finds every key
    /// already held, appends nothing and succeeds. Once this forget runs, that batch is on neither
    /// the log nor this set although that caller reported success. The loss is loud - the call
    /// whose append failed fails its step - and heals on the next sight of the identity, which asks
    /// the store afresh and appends the batch.
    pub(crate) fn forget(&self, ticket: &Ticket, kept: &[String]) {
        let mut generations = self.generations.lock().unwrap();
        let mut keys = self.keys.lock().unwrap();
        let Some((identity, epoch)) = &ticket.0 else {
            for key in kept {
                keys.remove(key);
            }
            return;
        };
        let Some(slot) = generations.slots.get_mut(identity) else {
            return;
        };
        if slot.epoch != *epoch {
            return;
        }
        for key in kept {
            keys.remove(key);
            slot.keys.remove(key);
        }
        if slot.keys.is_empty() {
            generations.slots.remove(identity);
        }
    }

    /// The generation tracked for `identity` with its keys in order, `None` when it has no slot.
    #[cfg(test)]
    pub(crate) fn tracked(&self, identity: &str) -> Option<(String, Vec<String>)> {
        self.generations
            .lock()
            .unwrap()
            .slots
            .get(identity)
            .map(|slot| {
                let mut keys: Vec<String> = slot.keys.iter().cloned().collect();
                keys.sort();
                (slot.generation.clone(), keys)
            })
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
