//! Ingest fixtures: the views of the latest-generation reference a test asserts against.

use rigger::eventstore::Event;
use std::collections::BTreeSet;

/// Every key of each identity's latest recorded generation among the events of `prior` whose
/// type is one of `types`: the one flattening of the latest-generation reference
/// ([`rigger::ingest::project_scoped_latest_generations`], spec 101). Read over the perception
/// types (spec 107), an identity whose latest recording is a ledger entry answers that entry's
/// key, and one whose latest is a keyed derived row the keys of that generation's rows.
pub fn latest_recorded_keys(prior: &[Event], types: &[&str]) -> BTreeSet<String> {
    rigger::ingest::project_scoped_latest_generations(prior, types)
        .into_values()
        .flat_map(|(_, keys)| keys)
        .collect()
}

/// THE KEYS-ONLY VIEW of the latest-generation reference over the derived index types of
/// `prior`: every key of each identity's latest recorded derived generation. The one flattening
/// of that reference every test boundary (the domain crate's tests, the conductor's, the root
/// suites) calls, never an inline copy.
pub fn reference_replay_keys(prior: &[Event]) -> BTreeSet<String> {
    latest_recorded_keys(prior, &rigger::ingest::DERIVED_INDEX_TYPES)
}
