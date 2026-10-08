//! Ingest fixtures: the views of the latest-generation reference a test asserts against.

use rigger::eventstore::Event;
use std::collections::BTreeSet;

/// Every replay key a recording of perception among `prior` carries that names its identity's
/// latest recorded generation ([`rigger::ingest::project_scoped_latest_generations`], spec 101):
/// an identity whose latest recording is a ledger entry answers the keys of that generation's
/// recordings, the entry's own among them, and one whose latest is a keyed derived row the keys
/// of that generation's rows. Collected here, test-side: production reads the generations alone
/// (spec 107).
pub fn latest_recorded_keys(prior: &[Event]) -> BTreeSet<String> {
    let latest = rigger::ingest::project_scoped_latest_generations(prior);
    prior
        .iter()
        .filter(|event| rigger::retention::PERCEPTION_TYPES.contains(&event.type_.as_str()))
        .filter_map(|event| event.meta.get(rigger::ingest::META_REPLAY_KEY))
        .filter(|key| {
            rigger::ingest::derived_key_parts(key).is_some_and(|(identity, generation)| {
                latest.get(identity).map(String::as_str) == Some(generation)
            })
        })
        .cloned()
        .collect()
}

/// THE KEYS-ONLY VIEW of the latest-generation reference over the derived index events of
/// `prior` alone: every key of each identity's latest recorded derived generation. The one
/// flattening of that reference every test boundary (the domain crate's tests, the conductor's,
/// the root suites) calls, never an inline copy.
pub fn reference_replay_keys(prior: &[Event]) -> BTreeSet<String> {
    let derived: Vec<Event> = prior
        .iter()
        .filter(|event| rigger::ingest::is_derived_index_type(&event.type_))
        .cloned()
        .collect();
    latest_recorded_keys(&derived)
}
