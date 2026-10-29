//! Ledger fixtures: what a run's sink records for a tree, one batch as a walk hands it to a sink,
//! and the view of a recorded entry a test compares - the one definition the root suites and the
//! conductor's own tests share.

use rigger::eventstore::Event;
use rigger::retention::GenerationIngested;

/// One ledger entry as a test compares it: its payload, its group and its replay key.
pub type EntryRecord = (GenerationIngested, String, String);

/// How many derived index events `events` carry.
pub fn derived_count(events: &[Event]) -> usize {
    events
        .iter()
        .filter(|event| rigger::ingest::is_derived_index_type(&event.type_))
        .count()
}

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

/// The replay key of each ledger entry a sink records for the tree at `root` as it stands and
/// nothing recorded, in walk order: one per batch the SHIPPED walk extracts, naming the batch's
/// identity, generation and event count.
#[cfg(feature = "symbols")]
pub fn walked_entry_keys(root: &std::path::Path) -> Vec<String> {
    recorded_entry_keys(&walked_entry_events(root, |_| String::new()))
}

/// The replay key of each ledger entry among `events`, in order, `events` asserted to hold no
/// derived index event: what a log holds of perception once both sinks record entries alone.
pub fn recorded_entry_keys(events: &[Event]) -> Vec<String> {
    assert_eq!(
        derived_count(events),
        0,
        "the log holds no derived index event"
    );
    entry_records(events)
        .into_iter()
        .map(|(_, _, key)| key)
        .collect()
}

/// What a ledger entry's replay key `<identity>@<generation>#<n>` names: the batch's identity,
/// its generation - both cut by the one key parser - and its event count.
pub fn entry_key_parts(key: &str) -> (String, String, usize) {
    let (identity, generation) = rigger::ingest::derived_key_parts(key)
        .expect("an entry's key names its identity and generation");
    let (_, events) = key
        .rsplit_once('#')
        .expect("an entry's key ends in its event count");
    (
        identity.to_string(),
        generation.to_string(),
        events.parse().expect("an event count"),
    )
}

/// What a sink records for the tree at `root` as it stands and nothing recorded, as
/// [`entry_records`] answers it: one entry per batch the SHIPPED walk extracts, in walk order,
/// its blob what `blob_of` answers for its file.
#[cfg(feature = "symbols")]
pub fn walked_entry_records(
    root: &std::path::Path,
    blob_of: impl Fn(&str) -> String,
) -> Vec<EntryRecord> {
    entry_records(&walked_entry_events(root, blob_of))
}

/// [`walked_entry_records`] with each entry's blob the id `git hash-object` gives the bytes the
/// tree holds at its file.
#[cfg(feature = "symbols")]
pub fn walked_git_entry_records(root: &std::path::Path) -> Vec<EntryRecord> {
    walked_entry_records(root, |file| super::git_hash_object(root, file, false))
}

/// The entry a sink records for each batch of the extraction tree's fixture (`WALKED`), in walk
/// order, as the event the entry's own constructor builds: its generation and flag the
/// fixture's, its event count the batch's, its blob what `blob_of` answers for its path.
pub fn fixture_entry_events(blob_of: impl Fn(&str) -> String) -> Vec<Event> {
    super::WALKED
        .iter()
        .map(|batch| {
            super::generation_ingested(
                batch.prefix,
                batch.path,
                batch.generation,
                &blob_of(batch.path),
                batch.excluded,
            )
            .event(batch.events.len())
        })
        .collect()
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

/// The batch the shipped whole-tree walk of `root` hands for `identity`.
#[cfg(feature = "symbols")]
pub fn handed_by_the_walk(root: &str, identity: &str) -> Handed {
    Handed::by(
        |sink| {
            rigger::ingest::ingest_project_batched(root, sink);
        },
        identity,
    )
}

/// Each ledger entry among `events`, in order, as a test compares it: its payload, its group and
/// its replay key. An entry carrying no group or no key reads as the empty string there.
pub fn entry_records(events: &[Event]) -> Vec<EntryRecord> {
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
/// while the graph holds each one's current generation. A store refuses a derived event, so
/// `store` is the pre-ledger store of its file (`PreLedgerStore`, the sqlite fixtures).
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
                (*event)
                    .clone()
                    .with_meta(rigger::ingest::META_REPLAY_KEY, key)
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

/// A GRAPH AHEAD OF THE RUN STREAM: record every batch the SHIPPED walk extracts from the tree at
/// `root` as a ledger entry on `stream` of `store`, a stream other than the run's, and fold each
/// with its batch into `graph`. The run stream's group lookup then answers no generation for any
/// identity while the graph holds each one's - the state
/// [`seed_pre_ledger_rows_without_a_group`] leaves, reached without a derived row, for a crate
/// whose tests cannot insert one.
#[cfg(feature = "symbols")]
pub fn seed_generations_on_another_stream(
    root: &std::path::Path,
    store: &dyn rigger::eventstore::EventStore,
    graph: &dyn rigger::contextgraph::Projection,
    stream: &str,
) {
    rigger::ingest::ingest_project_batched(root.to_str().unwrap(), |batch, excluded| {
        let (identity, generation) = rigger::ingest::derived_key_parts(&batch[0].0).unwrap();
        let (prefix, file) =
            rigger::retention::GenerationIngested::identity_parts(identity).unwrap();
        let entry = super::generation_ingested(prefix, file, generation, "", excluded);
        let events = batch.iter().map(|(_, event)| (*event).clone()).collect();
        record_folded_entry(store, graph, stream, &entry, events);
    });
}

/// Record `entry` on `stream` of `store` with `batch` as its batch, folding the two into `graph`,
/// and insist the batch folded.
#[cfg(feature = "symbols")]
pub fn record_folded_entry(
    store: &dyn rigger::eventstore::EventStore,
    graph: &dyn rigger::contextgraph::Projection,
    stream: &str,
    entry: &GenerationIngested,
    batch: Vec<Event>,
) {
    let done = rigger::ingest::folding_into(store, Some(graph), &|_| {})
        .append_entry_and_fold(stream, &entry.event(batch.len()), batch)
        .unwrap();
    assert_eq!(done.fold, rigger::contextgraph::Fold::Folded);
}

/// The generation `graph` holds for each of `identities`, in order.
pub fn held_generations(
    graph: &dyn rigger::contextgraph::Projection,
    identities: &[impl AsRef<str>],
) -> Vec<Option<String>> {
    identities
        .iter()
        .map(|identity| graph.current_generation(identity.as_ref()).unwrap())
        .collect()
}

/// The generation the group lookup of `store` answers on `stream` for each of `identities`, in
/// order.
pub fn logged_generations(
    store: &dyn rigger::eventstore::EventStore,
    stream: &str,
    identities: &[impl AsRef<str>],
) -> Vec<Option<String>> {
    identities
        .iter()
        .map(|identity| {
            rigger::ingest::latest_generation(store, stream, identity.as_ref()).unwrap()
        })
        .collect()
}

/// Every group lookup among `reads`, in call order: what the run's sink asked the store's group
/// lookup, the reads its memo proves itself current by ([`ledger_head_read`]) set aside.
pub fn group_lookups(reads: &[super::CountedRead]) -> Vec<super::CountedRead> {
    reads
        .iter()
        .filter(|read| matches!(read, super::CountedRead::LatestInGroup { .. }))
        .cloned()
        .collect()
}

/// THE MEMO'S HEAD READ (spec 107): the read of `stream`'s ledger head - its newest ledger
/// entry's revision - the run's sink makes for each batch it is handed, proving its memo current.
pub fn ledger_head_read(stream: &str) -> super::CountedRead {
    super::CountedRead::LastPosition {
        stream: stream.to_string(),
        event_type: rigger::retention::TYPE_GENERATION_INGESTED.to_string(),
    }
}

/// A REBUILD FROM THE TREE ALONE: [`rebuild_from_the_log`], one event to a committed batch,
/// re-extracting from the tree at `root`, with no object database to ask, each ledger entry its
/// fold asks a batch for. An entry whose file the tree holds at another generation resolves from no
/// source, so its identity is left behind.
#[cfg(feature = "symbols")]
pub fn rebuild_from_the_tree(graph_db: &std::path::Path, log: &[Event], root: &std::path::Path) {
    let root = root.to_path_buf();
    rebuild_from_the_log(
        graph_db,
        log,
        1,
        super::resolving(move |entry| rigger::ingest::resolve_entry(&root, entry, None)),
    );
}

/// A COLD REBUILD OF `log`: rebuild the graph file `graph_db` under the project `test`, owed, from
/// `log` in batches of at most `batch` events, each one committed transaction, re-extracting each
/// ledger entry its fold asks a batch for through the re-extraction `reextraction` builds for the
/// rebuild ([`super::resolving`]). It insists the rebuild ran.
pub fn rebuild_from_the_log(
    graph_db: &std::path::Path,
    log: &[Event],
    batch: usize,
    reextraction: impl Fn() -> Box<rigger::contextgraph::sqlite::Reextract<'static>>
        + Send
        + Sync
        + 'static,
) {
    use rigger::contextgraph::sqlite::{stream_past, Projector, RebuildSink};

    let rebuilt = Projector::open(graph_db.to_str().unwrap(), "test")
        .unwrap()
        .with_reextraction(reextraction)
        .rebuild(
            &Projector::lock_rebuild(graph_db.to_str().unwrap()).unwrap(),
            true,
            &mut |after, sink: &mut RebuildSink| stream_past(log, after, batch, sink),
            &mut |_| {},
        )
        .unwrap();
    assert!(rebuilt.is_some(), "premise: the rebuild ran");
}

/// The graph file `name` under `dir` rebuilt from `log` alone ([`rebuild_from_the_log`] in batches
/// of at most `batch` events, re-extracting no ledger entry), opened as its fold left it.
pub fn rebuilt_graph_file(
    dir: &std::path::Path,
    name: &str,
    log: &[Event],
    batch: usize,
) -> rigger::contextgraph::sqlite::Projector {
    rebuild_from_the_log(&dir.join(name), log, batch, super::resolving(|_| Ok(None)));
    super::graph_file_in(dir, name)
}

/// LEAVE `graph`, open on the graph file `graph_db`, OWING ITS REBUILD by an older fold rule: it
/// folds one decision, then the file records the rule before the current one, so every fold after
/// it is refused for that debt. The rule is stamped through the store's own opener, the one sqlite
/// connection every crate that mounts this fixture reaches.
pub fn owe_a_rebuild(graph: &dyn rigger::contextgraph::Projection, graph_db: &std::path::Path) {
    let decision = super::event_of("DecisionMade", super::decision_json("d-owed", "s", &[], ""));
    super::folds(graph, &[super::placed_at(decision, 1, 1)]);
    rigger_store_sqlite::sqlite::open_connection(graph_db.to_str().unwrap())
        .unwrap()
        .pragma_update(
            None,
            "user_version",
            rigger::contextgraph::sqlite::PROJECTION_VERSION - 1,
        )
        .unwrap();
    assert!(graph.rebuild_owed().unwrap());
}

/// The generation the walk of the tree at `root` keys the `gc` batch of `path` under now.
#[cfg(feature = "symbols")]
pub fn code_generation_now(root: &std::path::Path, path: &str) -> String {
    handed_by_the_walk(root.to_str().unwrap(), &format!("gc/{path}")).generation()
}

/// Record a ledger entry of the `gc` batch of `path` at `generation` on the run stream of
/// `store`, by a plain append that folds nothing: the log's side alone moves.
pub fn record_unfolded_entry(
    store: &dyn rigger::eventstore::EventStore,
    path: &str,
    generation: &str,
) {
    store
        .append(
            rigger::conductor::STREAM,
            rigger::eventstore::ExpectedRevision::Any,
            &[super::generation_ingested("gc", path, generation, "", false).event(1)],
        )
        .unwrap();
}
