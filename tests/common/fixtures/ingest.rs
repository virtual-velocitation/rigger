//! Ingest fixtures: the views of the derived index's reference a test asserts against.

use rigger::eventstore::Event;
use std::collections::BTreeSet;

/// THE KEYS-ONLY VIEW of the latest-generation reference
/// ([`rigger::ingest::project_scoped_latest_generations`], spec 101) over `prior`: every key of each
/// identity's latest recorded generation. The one flattening of that reference every test boundary
/// (the domain crate's tests, the conductor's, the root suites) calls, never an inline copy.
pub fn reference_replay_keys(prior: &[Event]) -> BTreeSet<String> {
    rigger::ingest::project_scoped_latest_generations(prior, &rigger::ingest::DERIVED_INDEX_TYPES)
        .into_values()
        .flat_map(|(_, keys)| keys)
        .collect()
}
