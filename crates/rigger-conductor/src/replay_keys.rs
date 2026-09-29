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

#[cfg(feature = "symbols")]
use crate::eventstore::Event;
#[cfg(feature = "symbols")]
use std::collections::HashMap;

/// The replay-key set and the per-identity generations it holds derived keys for.
pub(crate) struct ReplayKeys {
    #[cfg(feature = "symbols")]
    generations: Mutex<HashMap<String, Slot>>,
    keys: Mutex<HashSet<String>>,
}

/// One identity's tracked generation and the keys that generation contributed to the set.
#[cfg(feature = "symbols")]
struct Slot {
    generation: String,
    keys: HashSet<String>,
}

impl ReplayKeys {
    /// A set holding `keys`, tracking no generation.
    pub(crate) fn seeded(keys: HashSet<String>) -> Self {
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
    /// in batch order, each with its key.
    ///
    /// The FIRST time an identity is met, `first_sight` answers whether the batch is that
    /// identity's latest recorded generation (spec 101). It is asked under the generations lock,
    /// so the unseen check, the answer and the slot it installs are one step: a concurrent call
    /// meeting the same identity waits, then finds the slot. An unanswered lookup is this call's
    /// error and installs nothing. A recorded batch installs its keys, so none of it survives.
    ///
    /// A batch naming a generation other than the one its identity's slot tracks retires that
    /// generation's keys first (spec 86 criterion 3), so the fresh generation is never shadowed by
    /// a stale key that happens to hash identically. Then each event `rebuild` answers survives
    /// unless the set already holds its key; `rebuild` answering `None` skips the event without
    /// recording its key. A batch naming no identity asks nothing and tracks no generation: it is
    /// the plain dedup alone.
    pub(crate) fn install<E>(
        &self,
        keyed: &[(String, &Event)],
        first_sight: impl FnOnce() -> Result<bool, E>,
        mut rebuild: impl FnMut(&str, &Event) -> Option<Event>,
    ) -> Result<Vec<(String, Event)>, E> {
        let named = identity_generation(keyed);
        let mut generations = self.generations.lock().unwrap();
        let recorded = match named {
            Some((identity, _)) if !generations.contains_key(identity) => first_sight()?,
            _ => false,
        };
        let mut keys = self.keys.lock().unwrap();
        let mut slot = named.map(|(identity, generation)| {
            let slot = generations
                .entry(identity.to_string())
                .or_insert_with(|| Slot {
                    generation: generation.to_string(),
                    keys: HashSet::new(),
                });
            if slot.generation != generation {
                for stale in slot.keys.drain() {
                    keys.remove(&stale);
                }
                slot.generation = generation.to_string();
            }
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
        Ok(survivors)
    }

    /// Forget what one [`install`](ReplayKeys::install) of `keyed` recorded, after its append
    /// failed and recorded nothing: its `kept` keys (the survivors it returned) leave the set and
    /// its identity's slot, and the slot itself is removed only when it still names this batch's
    /// generation and is left holding no keys - so the next sight asks the store afresh and follows
    /// what the store holds. A slot a concurrent call has since moved to a newer generation is that
    /// call's, and is left exactly as it stands.
    ///
    /// THE IN-FLIGHT WINDOW: both locks are released across the append, so between the install and
    /// this forget a concurrent call emitting the SAME identity and generation finds every key
    /// already held, appends nothing and succeeds. Once this forget runs, that batch is on neither
    /// the log nor this set although that caller reported success. The loss is loud - the call
    /// whose append failed fails its step - and heals on the next sight of the identity, which asks
    /// the store afresh and appends the batch.
    pub(crate) fn forget(&self, keyed: &[(String, &Event)], kept: &[String]) {
        let mut generations = self.generations.lock().unwrap();
        let mut keys = self.keys.lock().unwrap();
        for key in kept {
            keys.remove(key);
        }
        let Some((identity, generation)) = identity_generation(keyed) else {
            return;
        };
        if let Some(slot) = generations.get_mut(identity) {
            if slot.generation == generation {
                for key in kept {
                    slot.keys.remove(key);
                }
                if slot.keys.is_empty() {
                    generations.remove(identity);
                }
            }
        }
    }

    /// The generation tracked for `identity` with its keys in order, `None` when it has no slot.
    #[cfg(test)]
    pub(crate) fn tracked(&self, identity: &str) -> Option<(String, Vec<String>)> {
        self.generations.lock().unwrap().get(identity).map(|slot| {
            let mut keys: Vec<String> = slot.keys.iter().cloned().collect();
            keys.sort();
            (slot.generation.clone(), keys)
        })
    }
}

#[cfg(all(test, feature = "symbols"))]
mod tests {
    use super::ReplayKeys;
    use crate::eventstore::Event;
    use std::collections::HashSet;

    const IDENTITY: &str = "gc/src/a.rs";

    fn batch(generation: &str, n: usize) -> Vec<(String, Event)> {
        (0..n)
            .map(|i| {
                (
                    format!("{IDENTITY}@{generation}#{i}"),
                    Event::new("T", vec![]),
                )
            })
            .collect()
    }

    fn keyed(batch: &[(String, Event)]) -> Vec<(String, &Event)> {
        batch.iter().map(|(key, ev)| (key.clone(), ev)).collect()
    }

    /// Install `batch` answering `recorded` at first sight, every event rebuilt as itself; the
    /// survivors' keys.
    fn install(set: &ReplayKeys, batch: &[(String, Event)], recorded: bool) -> Vec<String> {
        set.install(
            &keyed(batch),
            || Ok::<_, ()>(recorded),
            |_, ev| Some(ev.clone()),
        )
        .unwrap()
        .into_iter()
        .map(|(key, _)| key)
        .collect()
    }

    fn keys_of(batch: &[(String, Event)]) -> Vec<String> {
        batch.iter().map(|(key, _)| key.clone()).collect()
    }

    #[test]
    fn a_key_is_new_once_and_held_after() {
        let set = ReplayKeys::seeded(HashSet::from(["seeded".to_string()]));
        assert!(set.contains("seeded"));
        assert!(!set.insert("seeded"), "a seeded key is a replay");
        assert!(!set.contains("fresh"));
        assert!(set.insert("fresh"), "an unseen key is new work");
        assert!(!set.insert("fresh"), "and a replay after");
        assert!(set.contains("fresh"));
    }

    #[test]
    fn a_recorded_batch_survives_nothing_and_an_unrecorded_one_survives_whole() {
        let (h1, h2) = (batch("h1", 2), batch("h2", 2));
        let set = ReplayKeys::seeded(HashSet::new());
        assert_eq!(install(&set, &h1, true), Vec::<String>::new());
        assert_eq!(set.tracked(IDENTITY), Some(("h1".into(), keys_of(&h1))));

        let other = ReplayKeys::seeded(HashSet::new());
        assert_eq!(install(&other, &h2, false), keys_of(&h2));
        assert_eq!(other.tracked(IDENTITY), Some(("h2".into(), keys_of(&h2))));
    }

    #[test]
    fn an_unanswered_first_sight_installs_nothing_and_a_seen_identity_is_not_asked_again() {
        let h1 = batch("h1", 1);
        let set = ReplayKeys::seeded(HashSet::new());
        let unanswered = set.install(&keyed(&h1), || Err("down"), |_, ev| Some(ev.clone()));
        assert_eq!(unanswered.map(|s| s.len()), Err("down"));
        assert_eq!(set.tracked(IDENTITY), None);
        assert!(!set.contains(&h1[0].0));

        assert_eq!(install(&set, &h1, false), keys_of(&h1));
        let again = set.install(
            &keyed(&h1),
            || -> Result<bool, ()> { panic!("a seen identity is never looked up again") },
            |_, ev| Some(ev.clone()),
        );
        assert_eq!(again.map(|s| s.len()), Ok(0));
    }

    #[test]
    fn a_new_generation_retires_the_previous_ones_keys() {
        let (h1, h2) = (batch("h1", 2), batch("h2", 1));
        let set = ReplayKeys::seeded(HashSet::new());
        install(&set, &h1, true);
        assert_eq!(install(&set, &h2, false), keys_of(&h2));
        assert_eq!(set.tracked(IDENTITY), Some(("h2".into(), keys_of(&h2))));
        assert!(!set.contains(&h1[0].0) && !set.contains(&h1[1].0));
        assert_eq!(install(&set, &h1, false), keys_of(&h1), "a revert appends");
    }

    #[test]
    fn an_event_rebuild_skips_appends_nothing_and_records_no_key() {
        let h1 = batch("h1", 2);
        let set = ReplayKeys::seeded(HashSet::new());
        let survivors = set
            .install(
                &keyed(&h1),
                || Ok::<_, ()>(false),
                |key, ev| (!key.ends_with("#0")).then(|| ev.clone()),
            )
            .unwrap();
        assert_eq!(
            survivors.into_iter().map(|(k, _)| k).collect::<Vec<_>>(),
            [h1[1].0.clone()]
        );
        assert!(!set.contains(&h1[0].0));
        assert_eq!(
            set.tracked(IDENTITY),
            Some(("h1".into(), vec![h1[1].0.clone()]))
        );
    }

    #[test]
    fn forgetting_the_only_install_of_a_generation_removes_its_slot_and_keys() {
        let h1 = batch("h1", 2);
        let set = ReplayKeys::seeded(HashSet::new());
        let kept = install(&set, &h1, false);
        set.forget(&keyed(&h1), &kept);
        assert_eq!(set.tracked(IDENTITY), None);
        assert!(!set.contains(&h1[0].0) && !set.contains(&h1[1].0));
    }

    #[test]
    fn forgetting_leaves_the_keys_an_earlier_install_of_the_same_generation_holds() {
        let (first, whole) = (batch("h1", 1), batch("h1", 2));
        let set = ReplayKeys::seeded(HashSet::new());
        install(&set, &first, false);
        let kept = install(&set, &whole, false);
        assert_eq!(kept, [whole[1].0.clone()]);
        set.forget(&keyed(&whole), &kept);
        assert_eq!(set.tracked(IDENTITY), Some(("h1".into(), keys_of(&first))));
        assert!(set.contains(&whole[0].0) && !set.contains(&whole[1].0));
    }

    #[test]
    fn forgetting_leaves_a_slot_a_newer_generation_has_taken() {
        let (h2, h3) = (batch("h2", 2), batch("h3", 2));
        let set = ReplayKeys::seeded(HashSet::new());
        let kept = install(&set, &h2, false);
        install(&set, &h3, false);
        set.forget(&keyed(&h2), &kept);
        assert_eq!(set.tracked(IDENTITY), Some(("h3".into(), keys_of(&h3))));
        assert!(set.contains(&h3[0].0) && set.contains(&h3[1].0));
    }

    #[test]
    fn a_batch_naming_no_identity_is_the_plain_dedup_and_forgets_its_keys() {
        let ev = Event::new("T", vec![]);
        let unshaped = [("unshaped#0".to_string(), &ev)];
        let set = ReplayKeys::seeded(HashSet::new());
        let kept = set
            .install(
                &unshaped,
                || -> Result<bool, ()> { panic!("a batch naming no identity asks nothing") },
                |_, ev| Some(ev.clone()),
            )
            .unwrap();
        assert_eq!(kept.len(), 1);
        assert!(set.contains("unshaped#0"));
        set.forget(&unshaped, &["unshaped#0".to_string()]);
        assert!(!set.contains("unshaped#0"));
    }
}
