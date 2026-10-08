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

/// Each ledger entry among `events`, in order, as a test compares it: its payload, its group and
/// its replay key. An entry carrying no group or no key reads as the empty string there.
pub fn entry_records(
    events: &[Event],
) -> Vec<(rigger::retention::GenerationIngested, String, String)> {
    let meta = |event: &Event, name: &str| event.meta.get(name).cloned().unwrap_or_default();
    events
        .iter()
        .filter(|event| event.type_ == rigger::retention::TYPE_GENERATION_INGESTED)
        .map(|event| {
            (
                rigger::retention::GenerationIngested::parse(&event.data)
                    .expect("a ledger entry's payload parses"),
                meta(event, rigger::eventstore::META_GROUP),
                meta(event, rigger::ingest::META_REPLAY_KEY),
            )
        })
        .collect()
}
