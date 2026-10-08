//! Ledger fixtures: what a run's sink records for a tree, one batch as a walk hands it to a sink,
//! and the view of a recorded entry a test compares - the one definition the root suites and the
//! conductor's own tests share.

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
        let (prefix, file) =
            GenerationIngested::identity_parts(identity).expect("an identity is <prefix>/<file>");
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

/// One batch a walk handed its sink, owned, with its flag.
#[cfg(feature = "symbols")]
pub struct Handed {
    pub keyed: Vec<(String, Event)>,
    pub excluded: bool,
}

#[cfg(feature = "symbols")]
impl Handed {
    /// The batch of `identity` among those `walk` hands its sink.
    pub fn by(walk: impl FnOnce(&mut dyn rigger::ingest::BatchSink), identity: &str) -> Self {
        let mut found = None;
        walk(&mut |keyed: &[(String, &Event)], excluded: bool| {
            let named = keyed
                .first()
                .and_then(|(key, _)| rigger::ingest::derived_key_parts(key));
            if named.map(|(of, _)| of) == Some(identity) {
                found = Some(Handed {
                    keyed: keyed
                        .iter()
                        .map(|(key, event)| (key.clone(), (*event).clone()))
                        .collect(),
                    excluded,
                });
            }
        });
        found.unwrap_or_else(|| panic!("the walk hands a batch for {identity}"))
    }

    /// The generation the walk keyed the batch under.
    pub fn generation(&self) -> String {
        let (_, generation) = rigger::ingest::derived_key_parts(&self.keyed[0].0)
            .expect("a walk keys every batch under its identity and generation");
        generation.to_string()
    }

    /// The batch as a sink takes it.
    pub fn as_keyed(&self) -> Vec<(String, &Event)> {
        self.keyed
            .iter()
            .map(|(key, event)| (key.clone(), event))
            .collect()
    }
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

/// A STORE RECORDED BEFORE THE LEDGER AND BEFORE THE GROUP STAMP (spec 107, SINK OUTCOMES row
/// 13's fixture): record every batch the SHIPPED walk extracts from the tree at `root` into
/// `store`'s run stream as derived rows that carry their replay key and no group, and fold each
/// batch into `graph`. The group lookup then answers no generation for any identity of the tree
/// while the graph holds each one's current generation.
#[cfg(feature = "symbols")]
pub fn seed_pre_ledger_rows_without_a_group(
    root: &std::path::Path,
    store: &dyn rigger::eventstore::EventStore,
    graph: &dyn rigger::contextgraph::Projection,
) {
    let folding = rigger::ingest::folding_into(store, Some(graph), &|_| {});
    rigger::ingest::ingest_project_batched(root.to_str().unwrap(), |batch, _| {
        let rows: Vec<Event> = batch
            .iter()
            .map(|(key, event)| {
                let identity = rigger::ingest::derived_key_parts(key)
                    .expect("the walk keys every batch under its identity and generation")
                    .0;
                let mut row = rigger::ingest::keyed_derived_event((*event).clone(), key);
                assert_eq!(
                    row.meta.remove(rigger::eventstore::META_GROUP).as_deref(),
                    Some(identity)
                );
                row
            })
            .collect();
        let done = folding
            .append_and_fold(
                rigger::conductor::STREAM,
                rigger::eventstore::ExpectedRevision::Any,
                &rows,
            )
            .unwrap();
        assert_eq!(done.fold, rigger::contextgraph::Fold::Folded);
    });
}

/// The generation `graph` holds for each of `identities`, in order.
pub fn held_generations(
    graph: &dyn rigger::contextgraph::Projection,
    identities: &[&str],
) -> Vec<Option<String>> {
    identities
        .iter()
        .map(|identity| graph.current_generation(identity).unwrap())
        .collect()
}

/// The generation the group lookup of `store` answers on `stream` for each of `identities`, in
/// order.
pub fn logged_generations(
    store: &dyn rigger::eventstore::EventStore,
    stream: &str,
    identities: &[&str],
) -> Vec<Option<String>> {
    identities
        .iter()
        .map(|identity| rigger::ingest::latest_generation(store, stream, identity).unwrap())
        .collect()
}

/// A REBUILD FROM THE TREE ALONE: rebuild the graph file `graph_db` under the project `test`
/// from `log`, one event to a committed batch, re-extracting each ledger entry from the tree at
/// `root` with no object database to ask. It insists the rebuild ran. An entry whose file the
/// tree holds at another generation resolves from no source, so its identity is left behind.
#[cfg(feature = "symbols")]
pub fn rebuild_from_the_tree(graph_db: &std::path::Path, log: &[Event], root: &std::path::Path) {
    use rigger::contextgraph::sqlite::{stream_past, Projector, RebuildSink};

    let rebuilt = Projector::rebuild(
        &Projector::lock_rebuild(graph_db.to_str().unwrap()).unwrap(),
        "test",
        true,
        &mut |after, sink: &mut RebuildSink| stream_past(log, after, 1, sink),
        &mut |entry| rigger::ingest::resolve_entry(root, entry, None),
        &mut |_| {},
    )
    .unwrap();
    assert!(rebuilt.is_some(), "premise: the rebuild ran");
}

/// LEAVE `graph` OWING ITS REBUILD: the generic fold refuses a ledger entry and marks the graph
/// it refused, so every fold after it is refused for that debt.
pub fn owe_a_rebuild(graph: &dyn rigger::contextgraph::Projection) {
    use rigger::contextgraph::{wired, Fold};

    let mut stray = GenerationIngested {
        prefix: "gc".to_string(),
        file: "src/stray.rs".to_string(),
        generation: "h0".to_string(),
        blob: String::new(),
        excluded: false,
    }
    .event(1);
    stray.position = 1;
    assert_ne!(Fold::of(wired(Some(graph)), &stray), Fold::Folded);
    assert!(graph.rebuild_owed().unwrap());
}
