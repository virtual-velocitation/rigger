//! Ledger fixtures: what a run's sink records for a tree, and the view of a recorded entry a test
//! compares - the one definition the root suites and the conductor's own tests share.

use rigger::eventstore::Event;
use rigger::retention::GenerationIngested;

/// What the run's sink records for the tree at `root` as it stands and nothing recorded: one
/// ledger entry per batch the SHIPPED walk extracts, in walk order, each the event the entry's
/// own constructor builds - its generation the batch's, its blob what `blob_of` answers for its
/// file, its flag the walk's, its event count the batch's.
#[cfg(feature = "symbols")]
pub fn walked_entry_events(root: &std::path::Path, blob_of: impl Fn(&str) -> String) -> Vec<Event> {
    let mut out: Vec<Event> = Vec::new();
    rigger::ingest::ingest_project_batched(root.to_str().unwrap(), |batch, excluded| {
        let Some((key, _)) = batch.first() else {
            return;
        };
        let (identity, generation) = rigger::ingest::derived_key_parts(key)
            .expect("the walk keys every batch under its identity and generation");
        let (prefix, file) = identity
            .split_once('/')
            .expect("an identity is <prefix>/<file>");
        let entry = GenerationIngested {
            prefix: prefix.to_string(),
            file: file.to_string(),
            generation: generation.to_string(),
            blob: blob_of(file),
            excluded,
        };
        out.push(entry.event(batch.len()));
    });
    out
}

/// Each ledger entry among `events`, in order, as a test compares it: its payload, its group and
/// its replay key. An entry carrying no group or no key reads as the empty string there.
pub fn entry_records(events: &[Event]) -> Vec<(GenerationIngested, String, String)> {
    let meta = |event: &Event, name: &str| event.meta.get(name).cloned().unwrap_or_default();
    events
        .iter()
        .filter(|event| event.type_ == rigger::retention::TYPE_GENERATION_INGESTED)
        .map(|event| {
            (
                GenerationIngested::parse(&event.data).expect("a ledger entry's payload parses"),
                meta(event, rigger::eventstore::META_GROUP),
                meta(event, rigger::ingest::META_REPLAY_KEY),
            )
        })
        .collect()
}
