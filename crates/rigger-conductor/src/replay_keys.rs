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
/// EPOCH of the install that set this generation: minted afresh each time the slot is created or
/// switches generation, so an install that tracked a generation which has since moved away and
/// come back is told apart from the install that brought it back.
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
    /// in batch order, each with its key, and the [`Ticket`] a failed append hands to
    /// [`forget`](ReplayKeys::forget).
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
            let slot = slots.entry(identity.to_string()).or_insert_with(|| Slot {
                generation: generation.to_string(),
                keys: HashSet::new(),
                epoch: mint(minted),
            });
            if slot.generation != generation {
                for stale in slot.keys.drain() {
                    keys.remove(&stale);
                }
                slot.generation = generation.to_string();
                slot.epoch = mint(minted);
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

#[cfg(all(test, feature = "symbols"))]
mod tests {
    use super::{ReplayKeys, Ticket};
    use crate::eventstore::Event;
    use std::collections::HashSet;

    const IDENTITY: &str = "gc/src/a.rs";

    /// The `n` keyed events of [`IDENTITY`]'s batch at `generation`, each carrying `ev`.
    fn batch<'e>(generation: &str, n: usize, ev: &'e Event) -> Vec<(String, &'e Event)> {
        (0..n)
            .map(|i| (format!("{IDENTITY}@{generation}#{i}"), ev))
            .collect()
    }

    /// Install `batch` answering `recorded` at first sight, every event rebuilt as itself; the
    /// survivors' keys and the install's ticket.
    fn install(
        set: &ReplayKeys,
        batch: &[(String, &Event)],
        recorded: bool,
    ) -> (Vec<String>, Ticket) {
        let (survivors, ticket) = set
            .install(batch, || Ok::<_, ()>(recorded), |_, ev| Some(ev.clone()))
            .unwrap();
        (survivors.into_iter().map(|(key, _)| key).collect(), ticket)
    }

    fn keys_of(batch: &[(String, &Event)]) -> Vec<String> {
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
        let ev = Event::new("T", vec![]);
        let (h1, h2) = (batch("h1", 2, &ev), batch("h2", 2, &ev));
        let set = ReplayKeys::seeded(HashSet::new());
        assert_eq!(install(&set, &h1, true).0, Vec::<String>::new());
        assert_eq!(set.tracked(IDENTITY), Some(("h1".into(), keys_of(&h1))));

        let other = ReplayKeys::seeded(HashSet::new());
        assert_eq!(install(&other, &h2, false).0, keys_of(&h2));
        assert_eq!(other.tracked(IDENTITY), Some(("h2".into(), keys_of(&h2))));
    }

    #[test]
    fn an_unanswered_first_sight_installs_nothing_and_a_seen_identity_is_not_asked_again() {
        let ev = Event::new("T", vec![]);
        let h1 = batch("h1", 1, &ev);
        let set = ReplayKeys::seeded(HashSet::new());
        let unanswered = set.install(&h1, || Err("down"), |_, ev| Some(ev.clone()));
        assert_eq!(unanswered.map(|(s, _)| s.len()).err(), Some("down"));
        assert_eq!(set.tracked(IDENTITY), None);
        assert!(!set.contains(&h1[0].0));

        assert_eq!(install(&set, &h1, false).0, keys_of(&h1));
        let again = set.install(
            &h1,
            || -> Result<bool, ()> { panic!("a seen identity is never looked up again") },
            |_, ev| Some(ev.clone()),
        );
        assert_eq!(again.map(|(s, _)| s.len()).ok(), Some(0));
    }

    #[test]
    fn a_new_generation_retires_the_previous_ones_keys() {
        let ev = Event::new("T", vec![]);
        let (h1, h2) = (batch("h1", 2, &ev), batch("h2", 1, &ev));
        let set = ReplayKeys::seeded(HashSet::new());
        install(&set, &h1, true);
        assert_eq!(install(&set, &h2, false).0, keys_of(&h2));
        assert_eq!(set.tracked(IDENTITY), Some(("h2".into(), keys_of(&h2))));
        assert!(!set.contains(&h1[0].0) && !set.contains(&h1[1].0));
        assert_eq!(
            install(&set, &h1, false).0,
            keys_of(&h1),
            "a revert appends"
        );
    }

    #[test]
    fn an_event_rebuild_skips_appends_nothing_and_records_no_key() {
        let ev = Event::new("T", vec![]);
        let h1 = batch("h1", 2, &ev);
        let set = ReplayKeys::seeded(HashSet::new());
        let (survivors, _) = set
            .install(
                &h1,
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
        let ev = Event::new("T", vec![]);
        let h1 = batch("h1", 2, &ev);
        let set = ReplayKeys::seeded(HashSet::new());
        let (kept, ticket) = install(&set, &h1, false);
        set.forget(&ticket, &kept);
        assert_eq!(set.tracked(IDENTITY), None);
        assert!(!set.contains(&h1[0].0) && !set.contains(&h1[1].0));
    }

    #[test]
    fn forgetting_leaves_the_keys_an_earlier_install_of_the_same_generation_holds() {
        let ev = Event::new("T", vec![]);
        let (first, whole) = (batch("h1", 1, &ev), batch("h1", 2, &ev));
        let set = ReplayKeys::seeded(HashSet::new());
        install(&set, &first, false);
        let (kept, ticket) = install(&set, &whole, false);
        assert_eq!(kept, [whole[1].0.clone()]);
        set.forget(&ticket, &kept);
        assert_eq!(set.tracked(IDENTITY), Some(("h1".into(), keys_of(&first))));
        assert!(set.contains(&whole[0].0) && !set.contains(&whole[1].0));
    }

    #[test]
    fn forgetting_leaves_a_slot_a_newer_generation_has_taken() {
        let ev = Event::new("T", vec![]);
        let (h2, h3) = (batch("h2", 2, &ev), batch("h3", 2, &ev));
        let set = ReplayKeys::seeded(HashSet::new());
        let (kept, ticket) = install(&set, &h2, false);
        install(&set, &h3, false);
        set.forget(&ticket, &kept);
        assert_eq!(set.tracked(IDENTITY), Some(("h3".into(), keys_of(&h3))));
        assert!(set.contains(&h3[0].0) && set.contains(&h3[1].0));
    }

    #[test]
    fn forgetting_leaves_an_empty_slot_a_newer_generation_has_taken() {
        let ev = Event::new("T", vec![]);
        let (h2, h3) = (batch("h2", 2, &ev), batch("h3", 2, &ev));
        let set = ReplayKeys::seeded(HashSet::new());
        let (kept, ticket) = install(&set, &h2, false);
        let (none, _) = set
            .install(&h3, || Ok::<_, ()>(false), |_, _| None)
            .unwrap();
        assert_eq!(none.len(), 0);
        set.forget(&ticket, &kept);
        assert_eq!(set.tracked(IDENTITY), Some(("h3".into(), vec![])));
    }

    #[test]
    fn forgetting_an_install_whose_generation_moved_and_came_back_leaves_the_reinstall() {
        let ev = Event::new("T", vec![]);
        let (h2, h1) = (batch("h2", 2, &ev), batch("h1", 1, &ev));
        let set = ReplayKeys::seeded(HashSet::new());
        let (kept_a, ticket_a) = install(&set, &h2, false);
        install(&set, &h1, false);
        let (kept_d, _) = install(&set, &h2, false);
        assert_eq!(
            kept_d, kept_a,
            "the reinstall keeps the very same key strings"
        );
        set.forget(&ticket_a, &kept_a);
        assert_eq!(set.tracked(IDENTITY), Some(("h2".into(), keys_of(&h2))));
        assert!(set.contains(&h2[0].0) && set.contains(&h2[1].0));
        assert_eq!(install(&set, &h2, false).0, Vec::<String>::new());
    }

    #[test]
    fn a_batch_naming_no_identity_is_the_plain_dedup_and_forgets_its_keys() {
        let ev = Event::new("T", vec![]);
        let unshaped = [("unshaped#0".to_string(), &ev)];
        let set = ReplayKeys::seeded(HashSet::new());
        let (kept, ticket) = set
            .install(
                &unshaped,
                || -> Result<bool, ()> { panic!("a batch naming no identity asks nothing") },
                |_, ev| Some(ev.clone()),
            )
            .unwrap();
        assert_eq!(kept.len(), 1);
        assert!(set.contains("unshaped#0"));
        set.forget(&ticket, &["unshaped#0".to_string()]);
        assert!(!set.contains("unshaped#0"));
    }
}
