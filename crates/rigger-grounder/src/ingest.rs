//! Project-source ingest into the context graph: the ONE walk-and-content-key authority both
//! the live run (`conductor::RunCtx::ingest_project_batches`) and the standalone
//! `rigger graph build` entry share, so the content key an event is deduped under can never
//! drift between the two ingest entries.
//!
//! Each caller supplies its OWN emit sink - the run's replay-keyed, concurrency-safe
//! `emit_keyed`; the cold build's direct append-and-fold - because their mutation semantics
//! legitimately differ. What must NOT fork is the drift-prone part: the walk over the project's
//! per-file extraction batches, the `<prefix>/<file>@<hash>#<i>` content key, the keyed derived
//! event both record ([`keyed_derived_event`]), and the first-sight question that decides whether a
//! batch is already its identity's latest recorded generation ([`batch_is_latest_recorded`]).
//! Those are derived once, so the run and a cold `graph build` agree on every key and never
//! double-ingest one another's work.
//!
//! Symbols-gated: the walk lowers the tree through the `symbols` extraction pass, so the light
//! lane has nothing to ingest - a no-op that emits nothing, exactly as the run's ingest is a
//! no-op there.

use crate::contextgraph::{fold_loss_clause, wired, Fold, Projection};
use crate::eventstore::{
    Appended, Error, Event, EventBatchSink, EventStore, ExpectedRevision, Filter, GroupHead,
    Position, Revision, Subscription, TypeSelection,
};

pub use rigger_domain::ingest::*;

/// Exactly the events `appended` reports the store placed, each stamped with the position the
/// store issued for it (see [`FoldingStore::append_and_fold`]): what a fold of that append folds.
fn placed(stream: &str, events: &[Event], appended: &Appended) -> Result<Vec<Event>, Error> {
    // The port promises ONE slot per event handed in, and this authority folds by ZIPPING
    // the report against the batch - so a report of a different length is not a smaller
    // fold, it is a MISALIGNED one: every slot after the discrepancy names a different
    // event than the store meant, and the graph is then keyed by position onto the wrong
    // payload. `placed()` cannot see that (an index it cannot answer just yields nothing),
    // so it is checked here, once, where the zip happens.
    if appended.handed() != events.len() {
        return Err(Error::Backend(format!(
            "event store reported {} placement(s) for an append of {} event(s) to \
             {stream:?}: the report cannot name what was written",
            appended.handed(),
            events.len()
        )));
    }
    Ok(appended
        .placed()
        .filter_map(|(i, position)| {
            events.get(i).map(|e| {
                let mut e = e.clone();
                e.position = position;
                e
            })
        })
        .collect())
}

/// An [`EventStore`] that folds every event appended through it into the context graph, so a writer
/// that knows only the store port - a run's mint, a parked spawn, a recorded liveness fault, an
/// operator's resume - still leaves no event the graph's applied ledger misses. The append keeps
/// the caller's own expectation and goes to the log FIRST; only then is the graph opened, through
/// the `graph` opener the store was wired with (spec 101: a verb whose job is to append never opens
/// `graph.db` before its append), and the append folded at the positions the store issued. A fold
/// it could not make - the graph could not be had, owes its rebuild, or refused the write - is said
/// through `log`, never swallowed: the events are on the log whatever became of the fold. Wired
/// with no opener (an offline replay's isolated re-drive) it folds nothing by design and has
/// nothing to say. Every read passes through untouched.
pub struct FoldingStore<'a, O> {
    store: &'a dyn EventStore,
    graph: Option<O>,
    log: &'a (dyn Fn(&str) + Sync),
}

impl<'a, O> FoldingStore<'a, O> {
    pub fn new(
        store: &'a dyn EventStore,
        graph: Option<O>,
        log: &'a (dyn Fn(&str) + Sync),
    ) -> Self {
        Self { store, graph, log }
    }
}

/// A [`FoldingStore`] over a graph the caller already holds open, or over none: the one eager
/// wiring every writer that opened its graph first shares. Wired with none it folds nothing, as
/// [`FoldingStore::new`] with no opener does.
pub fn folding_into<'a>(
    store: &'a dyn EventStore,
    graph: Option<&'a dyn Projection>,
    log: &'a (dyn Fn(&str) + Sync),
) -> FoldingStore<
    'a,
    impl Fn() -> Result<&'a dyn Projection, crate::contextgraph::Error> + Send + Sync + 'a,
> {
    FoldingStore::new(
        store,
        graph.map(|g| move || crate::contextgraph::wired(Some(g))),
        log,
    )
}

impl<'g, O, G> FoldingStore<'_, O>
where
    O: Fn() -> Result<G, crate::contextgraph::Error> + Send + Sync,
    G: std::ops::Deref<Target = dyn Projection + 'g>,
{
    /// THE ONE APPEND-THEN-FOLD BODY: append `events` to `stream` under the caller's `expected`
    /// revision in ONE store append, then fold exactly what the store placed, at the positions it
    /// issued, in ONE graph transaction - the batched-fold cadence spec 49 needs (one store
    /// transaction per file's batch, not per event). The append goes to the log first and the
    /// graph is opened only after it (spec 101: a verb whose job is to append never opens
    /// `graph.db` before its append). A fold failure never fails the append, which already landed
    /// durably: it is returned as the batch's [`Fold`] beside the store's own report, for the caller
    /// to report. A store wired with no graph folds nothing and says so in that [`Fold`]. An empty
    /// batch and an unmet expectation are the store's to answer, never this body's.
    ///
    /// # Every folded event is stamped with the position THE STORE ISSUED
    ///
    /// This folds exactly the events [`Appended::placed`] names, at the positions the store
    /// reported, and derives no position of its own. Computing them arithmetically as
    /// `base = last + 1 - n` is unsound twice over: an append may write FEWER events than it was
    /// handed (a store may recognise an event as already recorded), and the port has never
    /// promised a batch lands at CONSECUTIVE positions - only distinct, strictly increasing ones,
    /// which a backend whose position is a byte offset satisfies with gaps. The graph's applied
    /// ledger is keyed BY position, so a wrong one marks a location applied forever and silently
    /// swallows the genuine event recorded there. A suppressed event needs no fold: it folded when
    /// its content was first recorded.
    ///
    /// Every batched append-then-fold is this body - the run's keyed emit and every other run
    /// event through [`EventStore::append`] below, a cold `rigger graph build`, the offline graph
    /// passes and `rigger reset --runs` through this method - so the batching and the fold can
    /// never diverge between them. Two single-event folds are not: `rigger emit`
    /// (`mcpserver::emit_event`) and `rigger result` (`fold_recorded_result`) each append their one
    /// event through the store and fold it through [`Fold::of`]. It is deliberately NOT
    /// `symbols`-gated: it only moves events through the store and graph ports, which both feature
    /// lanes compile.
    pub fn append_and_fold(
        &self,
        stream: &str,
        expected: ExpectedRevision,
        events: &[Event],
    ) -> Result<AppendedAndFolded, Error> {
        let appended = self.store.append(stream, expected, events)?;
        let placed = placed(stream, events, &appended)?;
        let fold = match &self.graph {
            Some(open) => Fold::of_batch(open, &placed),
            None => Fold::of_batch(|| wired(None), &placed),
        };
        Ok(AppendedAndFolded { appended, fold })
    }
}

impl<'g, O, G> EventStore for FoldingStore<'_, O>
where
    O: Fn() -> Result<G, crate::contextgraph::Error> + Send + Sync,
    G: std::ops::Deref<Target = dyn Projection + 'g>,
{
    fn append(
        &self,
        stream: &str,
        expected: ExpectedRevision,
        events: &[Event],
    ) -> Result<Appended, Error> {
        let done = self.append_and_fold(stream, expected, events)?;
        // A store wired with no graph folds nothing by design and has nothing to say.
        if self.graph.is_some() && done.fold != Fold::Folded {
            (self.log)(&format!(
                "rigger: recorded {} run event(s){}",
                events.len(),
                fold_loss_clause(&done.fold)
            ));
        }
        Ok(done.appended)
    }

    fn read_stream(
        &self,
        stream: &str,
        from: Revision,
        dir: crate::eventstore::Direction,
    ) -> Result<Vec<Event>, Error> {
        self.store.read_stream(stream, from, dir)
    }

    fn read_all(
        &self,
        from: Position,
        dir: crate::eventstore::Direction,
        filter: &Filter,
    ) -> Result<Vec<Event>, Error> {
        self.store.read_all(from, dir, filter)
    }

    fn subscribe_all(&self, from: Position, filter: &Filter) -> Result<Subscription, Error> {
        self.store.subscribe_all(from, filter)
    }

    fn subscribe_stream(&self, stream: &str, from: Revision) -> Result<Subscription, Error> {
        self.store.subscribe_stream(stream, from)
    }

    fn last_position(&self, stream: &str, event_type: &str) -> Result<Option<Revision>, Error> {
        self.store.last_position(stream, event_type)
    }

    fn read_stream_typed(
        &self,
        stream: &str,
        from: Revision,
        selection: TypeSelection,
    ) -> Result<Vec<Event>, Error> {
        self.store.read_stream_typed(stream, from, selection)
    }

    fn read_stream_positions(
        &self,
        stream: &str,
        batch: usize,
        sink: &mut dyn FnMut(&[Position]) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.store.read_stream_positions(stream, batch, sink)
    }

    fn read_stream_batched(
        &self,
        stream: &str,
        from: Revision,
        batch: usize,
        sink: &mut EventBatchSink,
    ) -> Result<(), Error> {
        self.store.read_stream_batched(stream, from, batch, sink)
    }

    fn latest_in_group(&self, stream: &str, group: &str) -> Result<Option<GroupHead>, Error> {
        self.store.latest_in_group(stream, group)
    }
}

/// What [`FoldingStore::append_and_fold`] did: the store's own report of what it wrote, and what became
/// of folding it into the context graph - which the caller reports, never drops.
#[must_use = "a fold that is not reported is a fold that can be silently lost"]
#[derive(Debug)]
pub struct AppendedAndFolded {
    pub appended: Appended,
    pub fold: Fold,
}

/// What a walk did, reported back to the caller. `batches_emitted` counts the file batches the walk
/// handed to `emit` (code, design, and the workflow definition). `workers_engaged` is how many
/// parse-worker threads actually ran the code half: `> 1` proves the parse fanned across cores, and
/// it is an HONEST count (the distinct threads that ran), so a serial walk (width 1) reports
/// exactly 1.
#[cfg(feature = "symbols")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IngestStats {
    pub batches_emitted: usize,
    pub workers_engaged: usize,
}

/// Walk the project tree at `root` and hand `on_batch` each file's WHOLE keyed batch of the
/// extraction events the code (spec 29a), design (spec 29b), and workflow-definition (spec 92
/// criterion 2) passes emit: the file's events, each paired with its deterministic content key
/// `<prefix>/<file>@<hash>#<i>` (`gc` for code, `gd` for design, `gw` for the workflow
/// definition), in `#i` order. The key is a pure function
/// of the batch's bytes ALONE, so the same content always yields the same keys and different
/// content always yields different ones. A key is therefore a CONTENT GENERATION of a file, not a
/// mark that the file has been seen: whether a given key is redundant is a question about the
/// file's LATEST recorded generation ([`batch_is_latest_recorded`] answers it), which is why a
/// file reverted to content it held earlier re-emits its whole batch even though every one of its
/// keys is already in the log. This function owns only the walk and the keying; the sink decides
/// what a key MEANS (append-and-fold, or skip a replay), so the mutation authority stays with the
/// caller. A sink appends the file's batch in ONE store append and folds it in ONE graph
/// transaction (via [`FoldingStore::append_and_fold`]) - the batched-fold cadence spec 49 needs, since the
/// measured cold-build throughput was transaction-cadence bound.
///
/// "Content" here is the batch this walk LOWERED, which is not always the file on disk, and the two
/// halves differ: the design half reads the live tree, while the code half reuses the `symbols`
/// grounder's PERSISTED index when the project has one (see [`walk_batches`]) and derives its events
/// from the indexed symbols without reading the file. So on such a project the keys track that
/// index's view - including for a path the tree no longer holds but the index still lists, which
/// this walk therefore still emits a batch for.
///
/// The code half's per-file parse/lower fans across a default-sized worker pool (one worker per
/// logical core), but the EMIT stays in sorted file-path order - parallelism is observationally
/// invisible. The returned [`IngestStats`] is informational.
#[cfg(feature = "symbols")]
pub fn ingest_project_batched(root: &str, on_batch: impl BatchSink) -> IngestStats {
    ingest_project_batched_paced(root, crate::parallel::default_workers(), on_batch)
}

/// [`ingest_project_batched`] at a chosen parse width. The code half (spec 29a) parses/lowers its
/// files across up to `workers` threads yet EMITS them in the index's sorted file-path order, so
/// the batch sequence a caller's sink observes is byte-identical to a serial walk's regardless of
/// scheduling (the rebuild-byte-identical discipline). `workers <= 1` runs the lowering inline: it
/// IS the serial walk a wider walk is proven byte-identical against - the same code path, not a
/// hand-rolled twin. Parse width changes only the code half's parallelism (criterion 1), never the
/// batching: the same files still emit as the same per-file batches. The design half (spec 29b)
/// stays serial; its walk lives in `design/events.rs`.
#[cfg(feature = "symbols")]
pub fn ingest_project_batched_paced(
    root: &str,
    workers: usize,
    on_batch: impl BatchSink,
) -> IngestStats {
    walk_batches(root, workers, on_batch)
}

/// The walk behind [`ingest_project_batched_paced`]: parse/lower the project at `root` and hand
/// each file's WHOLE keyed batch to `on_batch`, in sorted file-path order (the code half first,
/// then the design half, then the workflow-definition half), each batch in `#i` order.
#[cfg(feature = "symbols")]
fn walk_batches(root: &str, workers: usize, mut on_batch: impl BatchSink) -> IngestStats {
    let mut batches_emitted = 0usize;
    // The code half (spec 29a): parallel parse feeds this ordered emit. Reuses the `symbols`
    // grounder's persisted index when present (no re-parse), so in a live run this is a cheap read of
    // what the grounder already built - not a second whole-tree parse.
    let (code_batches, workers_engaged) =
        crate::grounder::symbols::events::project_batches_paced(root, workers);
    for (file, batch) in &code_batches {
        key_batch("gc", file, batch, &mut on_batch);
        batches_emitted += 1;
    }
    // The design half (spec 29b): the project's design docs and inline source rationale, serial.
    let design_batches = crate::grounder::design::events::project_batches(root);
    for (file, batch) in &design_batches {
        key_batch("gd", file, batch, &mut on_batch);
        batches_emitted += 1;
    }
    // The workflow-DEFINITION half (spec 92 criterion 2): `.rigger/workflow.yml`'s stages, gates
    // and agent roles, its own `gw` identity so this ONE file's generation can never collide with
    // (or retire) a same-named code/design batch - a project could, in principle, have a source
    // file at that same relative path under a different prefix. One batch at most (the file is
    // either fully readable and parseable, or it contributes nothing - see
    // `workflowdef::project_batches`'s own doc), so this loop runs at most once.
    let workflowdef_batches = crate::grounder::workflowdef::project_batches(root);
    for (file, batch) in &workflowdef_batches {
        key_batch("gw", file, batch, &mut on_batch);
        batches_emitted += 1;
    }
    IngestStats {
        batches_emitted,
        workers_engaged,
    }
}

/// [`ingest_project_batched`] SCOPED to exactly the NAMED `files` (spec 92, FRESH ON EVERY
/// INTEGRATION), rather than walking the whole project: the property an integration's OWN reindex
/// needs, bounded by the merge's OWN file list (Design/Constraints Walk: "the reindex is bounded by
/// the merge's file list"), never the project's total file count. Reuses the SAME per-file lowering
/// and keying as the whole-project walk (`crate::grounder::symbols::events::file_batches` and
/// [`key_batch`], the identical authority [`walk_batches`]'s code half calls) - never a second
/// lowering path - so a named file's scoped batch is byte-identical to what a full walk would
/// produce for it, and the content key an event is deduped under can never drift between the two
/// entries.
///
/// CODE ONLY (the `gc/` prefix): the design-intent half (`gd/`, spec 29b) stays with the
/// whole-project walk - this scoped entry exists for the code-graph freshness an integration's
/// reindex is answerable for, mirroring the EXISTING `Grounder::reindex` it runs alongside (which is
/// also code-only), not a second, independently-scoped design-intent freshness this spec does not
/// own.
#[cfg(feature = "symbols")]
pub fn ingest_files_batched(
    root: &str,
    files: &[String],
    mut on_batch: impl BatchSink,
) -> IngestStats {
    let batches = crate::grounder::symbols::events::file_batches(root, files);
    let mut batches_emitted = 0usize;
    for (file, batch) in &batches {
        key_batch("gc", file, batch, &mut on_batch);
        batches_emitted += 1;
    }
    IngestStats {
        batches_emitted,
        workers_engaged: 1,
    }
}

/// Light lane: no extraction pass is compiled, so there is nothing to walk - a no-op that hands the
/// sink no batches, mirroring [`ingest_project_batched`]'s own light-lane stub.
#[cfg(not(feature = "symbols"))]
pub fn ingest_files_batched(_root: &str, _files: &[String], _on_batch: impl BatchSink) {}

/// Sampled files whose CURRENT code extraction disagrees with what `graph.db` has recorded as their
/// latest `gc/` generation (spec 92, FRESH ON EVERY INTEGRATION) - `rigger validate`'s graph INDEX
/// LAG advisory. The graph-specific analogue of `grounder::symbols::staleness` (spec 68): the same
/// cost-bounded SAMPLE shape (the caller bounds `files` - never a full-tree scan lives here), but
/// measured against the GRAPH's own recorded generations (via [`project_scoped_latest_generations`]
/// over `prior`, the project's own event stream) rather than the symbols index's separately-
/// persisted hash column. The two stores can drift independently of one another - a `graph.db` built
/// before this spec's integration-time reindex existed, or a project whose symbols index was rebuilt
/// out-of-band - so the symbols index agreeing with the tree is never taken as proof the graph does
/// too; this reads the graph's own recording directly.
///
/// A file is FRESH when re-extracting it (through the SAME [`ingest_files_batched`] authority the
/// live conductor reindexes through) yields EXACTLY the key set `graph.db`'s latest `gc/<file>`
/// generation already recorded - same content, same event count, same order (the walk is
/// deterministic by construction, so an honest match is exact, never approximate). A file the graph
/// has NEVER recorded a generation for at all counts as lagging only when its current extraction is
/// non-empty (a genuinely new file the graph has not yet ingested - the coverage question criterion
/// 2 owns, not double-counted as this criterion's lag). Returns the subset of `files` that disagree;
/// `[]` means every sampled file agrees with the graph's own recording, as far as the sample can
/// tell - zero lag.
#[cfg(feature = "symbols")]
pub fn graph_index_lag(root: &str, prior: &[Event], files: &[String]) -> Vec<String> {
    let latest = project_scoped_latest_generations(prior);
    files
        .iter()
        .filter(|file| {
            let identity = format!("gc/{file}");
            let mut current_keys: Vec<String> = Vec::new();
            let scoped = std::slice::from_ref(*file);
            let _ = ingest_files_batched(root, scoped, |keyed| {
                current_keys.extend(keyed.iter().map(|(k, _)| k.clone()));
            });
            match latest.get(&identity) {
                None => !current_keys.is_empty(),
                Some((_hash, recorded_keys)) => {
                    let recorded: std::collections::BTreeSet<&String> =
                        recorded_keys.iter().collect();
                    let current: std::collections::BTreeSet<&String> =
                        current_keys.iter().collect();
                    recorded != current
                }
            }
        })
        .cloned()
        .collect()
}

/// Light lane: no extraction pass is compiled, so no file can ever be extracted - nothing to compare
/// against, and nothing is ever reported as lagging (mirrors [`ingest_files_batched`]'s own no-op).
#[cfg(not(feature = "symbols"))]
pub fn graph_index_lag(_root: &str, _prior: &[Event], _files: &[String]) -> Vec<String> {
    Vec::new()
}

/// Cost-bounded SAMPLE size for [`graph_index_lag_sample`] - mirrors
/// `grounder::symbols::STALENESS_SAMPLE_SIZE`'s own bound: a fixed, small, deterministic sample
/// keeps `rigger validate`'s graph index-lag advisory O(sample), never O(every file the graph has
/// ever recorded a generation for).
#[cfg(feature = "symbols")]
const GRAPH_INDEX_LAG_SAMPLE_SIZE: usize = 8;

/// `rigger validate`'s GRAPH INDEX LAG sample (spec 92, FRESH ON EVERY INTEGRATION): the bounded,
/// deterministic candidate list [`graph_index_lag`] is checked against, derived FROM `prior`
/// itself rather than a caller-supplied file list - so validate needs nothing but the project's own
/// event stream and its working tree, exactly like every other validate advisory.
///
/// Candidates are every file identity `prior`'s derived stream has recorded a `gc/` generation for
/// (via [`project_scoped_latest_generations`]) that STILL EXISTS on disk right now, sorted for
/// determinism, then truncated to [`GRAPH_INDEX_LAG_SAMPLE_SIZE`] - mirroring
/// `grounder::symbols::staleness`'s own sorted-intersection-then-take sampling shape. Two kinds of
/// file are deliberately left OUT of the candidate set, not merely filtered from the result:
///
/// - a file the graph has NEVER recorded (present on disk, absent from `prior`) - that is the
///   COVERAGE question (criterion 2's), never double-counted as this advisory's lag;
/// - a file the graph recorded that no longer exists on disk - an integration's own reindex
///   retires it directly through the boundary-sentinel supersession (Design/Constraints Walk: "a
///   file deleted by the integration - its entities are retired through the existing supersession,
///   not left dangling"), so this bounded sample has nothing useful to re-check for it.
///
/// Returns `[]` when there is nothing to sample (an empty `prior`, or every candidate already
/// pruned by the two rules above) or when every sampled file agrees with the graph's own recording -
/// zero lag, as far as the sample can tell.
#[cfg(feature = "symbols")]
pub fn graph_index_lag_sample(root: &str, prior: &[Event]) -> Vec<String> {
    let root_path = std::path::Path::new(root);
    let latest = project_scoped_latest_generations(prior);
    let mut candidates: Vec<String> = latest
        .keys()
        .filter_map(|identity| identity.strip_prefix("gc/"))
        .filter(|file| root_path.join(file).is_file())
        .map(str::to_string)
        .collect();
    candidates.sort();
    candidates.truncate(GRAPH_INDEX_LAG_SAMPLE_SIZE);
    graph_index_lag(root, prior, &candidates)
}

/// Light lane: no extraction pass is compiled, so nothing can ever disagree (mirrors
/// [`graph_index_lag`]'s own light-lane stub).
#[cfg(not(feature = "symbols"))]
pub fn graph_index_lag_sample(_root: &str, _prior: &[Event]) -> Vec<String> {
    Vec::new()
}

/// Key one file's batch under `<prefix>/<file>@<hash>#<i>` and hand the WHOLE keyed batch to
/// `on_batch` at once. `hash` fingerprints the WHOLE batch's bytes with the SAME line-ending-
/// normalized content primitive the symbols reindex freshening keys on (reused, not a fresh copy, so
/// the change-detection key is one content-identity authority), so every event of a file shares one
/// `<hash>`. The batch bytes are JSON the emit pass just serialized, so they are valid UTF-8.
#[cfg(feature = "symbols")]
fn key_batch(prefix: &str, file: &str, batch: &[Event], on_batch: &mut impl BatchSink) {
    let concat: String = batch
        .iter()
        .filter_map(|e| std::str::from_utf8(&e.data).ok())
        .collect();
    let hash = crate::grounder::symbols::store::content_hash(&concat);
    let keyed: Vec<(String, &Event)> = batch
        .iter()
        .enumerate()
        .map(|(i, ev)| (format!("{prefix}/{file}@{hash}#{i}"), ev))
        .collect();
    on_batch(&keyed);
}

/// The light lane compiles no extraction pass, so there is nothing to walk - a no-op that hands the
/// sink no batches. `graph build` still opens (creating) the store and degrades to an empty graph,
/// never an error, in either lane (the batched append-and-fold kernel above stays compiled in both).
#[cfg(not(feature = "symbols"))]
pub fn ingest_project_batched(_root: &str, _on_batch: impl BatchSink) {}

#[cfg(all(test, feature = "symbols"))]
mod tests {
    use super::{ingest_project_batched_paced, IngestStats};

    /// Drive a walk at `workers` width and capture the exact `(key, type, data)` triples the sink
    /// sees, in emit order - the observable the byte-identical contract is defined over.
    fn walk(root: &str, workers: usize) -> (Vec<(String, String, Vec<u8>)>, IngestStats) {
        let mut seq: Vec<(String, String, Vec<u8>)> = Vec::new();
        let stats = ingest_project_batched_paced(root, workers, |batch| {
            for (key, ev) in batch {
                seq.push((key.to_string(), ev.type_.clone(), ev.data.clone()));
            }
        });
        (seq, stats)
    }

    #[test]
    fn parallel_parse_emits_the_byte_identical_sequence_a_serial_walk_would() {
        // Criterion 1: ingesting a multi-file fixture at width 8 (parallel parse) must engage more
        // than one parse worker AND emit the SAME event sequence - same keys, types, and bytes, in
        // the SAME sorted-file-path order - that a serial walk (width 1) emits. The serial walk is
        // the SAME production function at width 1 (`map_ordered` short-circuits to an inline map), so
        // determinism is proven against the one code path, not a hand-rolled twin.
        let dir = tempfile::tempdir().unwrap();
        for i in 0..6 {
            std::fs::write(
                dir.path().join(format!("m{i}.rs")),
                format!("fn f{i}() {{}}\nfn g{i}() {{ f{i}(); }}\n"),
            )
            .unwrap();
        }
        let root = dir.path().to_str().unwrap();

        let (serial, serial_stats) = walk(root, 1);
        let (parallel, parallel_stats) = walk(root, 8);

        assert!(
            !serial.is_empty(),
            "the multi-file fixture emits code-ingest events"
        );
        assert_eq!(
            serial, parallel,
            "parallel parse feeds an ordered emit: the width-8 sequence is byte-identical to serial"
        );

        // Sorted file-path order: the code (`gc/`) keys name files in ascending path order.
        let gc_files: Vec<&str> = parallel
            .iter()
            .filter_map(|(k, _, _)| k.strip_prefix("gc/"))
            .filter_map(|rest| rest.split('@').next())
            .collect();
        let mut sorted = gc_files.clone();
        sorted.sort_unstable();
        assert_eq!(
            gc_files, sorted,
            "code batches emit in sorted file-path order; got {gc_files:?}"
        );

        // Stable batch keys: the two walks agree on every key (the content key is a pure function of
        // the batch bytes, independent of walk width).
        let keys = |seq: &[(String, String, Vec<u8>)]| -> Vec<String> {
            seq.iter().map(|(k, _, _)| k.clone()).collect()
        };
        assert_eq!(
            keys(&serial),
            keys(&parallel),
            "the batch keys are stable across walk width"
        );

        // Engagement: width 1 is the serial oracle (one worker); width 8 over six files fans out.
        assert_eq!(
            serial_stats.workers_engaged, 1,
            "width 1 runs inline: exactly one engaged worker"
        );
        assert!(
            parallel_stats.workers_engaged > 1,
            "width 8 over a six-file fixture engages more than one parse worker; got {}",
            parallel_stats.workers_engaged
        );
        assert_eq!(
            serial_stats.batches_emitted, parallel_stats.batches_emitted,
            "both widths emit the same number of file batches"
        );
        assert!(
            parallel_stats.batches_emitted >= 6,
            "each of the six source files contributes a code batch; got {}",
            parallel_stats.batches_emitted
        );
    }

    /// Spec 92 criterion 2: the workflow-definition pass rides this SAME shared walk as the code
    /// and design halves, under its own `gw` prefix, so a live run's ingest and a cold `rigger
    /// graph build` both pick up `.rigger/workflow.yml` with no separate wiring at either call
    /// site - proving the pipeline WIRING, not just `workflowdef`'s own standalone extraction.
    #[test]
    fn the_walk_ingests_the_workflow_definition_alongside_code_and_design() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn kept() {}\n").unwrap();
        std::fs::create_dir_all(dir.path().join(".rigger")).unwrap();
        std::fs::write(
            dir.path().join(".rigger").join("workflow.yml"),
            "stages:\n  implement:\n    agent: rust-engineer\n    gates: [fmt]\n\ngates:\n  fmt: { run: \"cargo fmt --check\" }\n",
        )
        .unwrap();
        let root = dir.path().to_str().unwrap();

        let (seq, stats) = walk(root, 1);
        assert!(
            seq.iter()
                .any(|(k, _, _)| k.starts_with("gw/.rigger/workflow.yml@")),
            "the workflow-definition batch must ride the shared walk under its own gw prefix, \
             got keys {:?}",
            seq.iter().map(|(k, _, _)| k).collect::<Vec<_>>()
        );
        assert!(
            seq.iter().any(|(k, t, _)| k.starts_with("gc/")
                && t == crate::contextgraph::TYPE_CODE_ENTITY_EXTRACTED),
            "the code half must still ingest alongside it, got {:?}",
            seq.iter().map(|(k, t, _)| (k, t)).collect::<Vec<_>>()
        );
        // Pins the shared `batches_emitted` accumulator across BOTH halves: this fixture has no
        // design-intent doc, so the count is exactly the one code batch (a.rs) plus the one
        // workflow-definition batch. An accumulator arm that stops advancing the count (a no-op
        // update rather than `+= 1`) on the workflow-definition loop specifically would still
        // pass every other assertion here while silently undercounting by one.
        assert_eq!(
            stats.batches_emitted, 2,
            "one code batch (a.rs) plus one workflow-definition batch (.rigger/workflow.yml) \
             must both advance the shared batch count; got {}",
            stats.batches_emitted
        );
    }

    /// Criterion 4 (the SCOPED WALK): the ingest walk is scoped to the project's own sources. A
    /// fixture tree with (a) an in-root source file the project keeps, (b) a directory the
    /// project's OWN `.gitignore` excludes, (c) the VCS metadata directory `.git`, (d) rigger's
    /// runtime directory `.rigger`, and (e) a symlink escaping the root, must ingest EVERY in-root
    /// source file and NONE of the excluded paths - so the graph grows no cluster for tooling,
    /// ignored, or out-of-root paths. This owns the walk scope and root confinement: the scoping
    /// rides the ONE shared `walk_guarded` authority, so proving it here proves it for every ingest
    /// half (code and design) that walk drives.
    #[test]
    fn ingest_scopes_the_walk_to_the_project_and_never_escapes_the_root() {
        use std::collections::BTreeSet;
        let root_dir = tempfile::tempdir().unwrap();
        let root = root_dir.path();
        // (a) an in-root source file the walk MUST ingest.
        std::fs::write(root.join("keep.rs"), "fn kept() {}\n").unwrap();
        // (b) a build directory the project declares not-source via its OWN version-control
        // ignore rules (a real `.gitignore` at the root naming `build/`).
        std::fs::write(root.join(".gitignore"), "build/\n").unwrap();
        std::fs::create_dir(root.join("build")).unwrap();
        std::fs::write(root.join("build").join("gen.rs"), "fn generated() {}\n").unwrap();
        // (c) the VCS metadata directory and (d) rigger's runtime directory - never source.
        std::fs::create_dir(root.join(".git")).unwrap();
        std::fs::write(root.join(".git").join("hook.rs"), "fn vcs_internal() {}\n").unwrap();
        std::fs::create_dir(root.join(".rigger")).unwrap();
        std::fs::write(
            root.join(".rigger").join("state.rs"),
            "fn runtime_state() {}\n",
        )
        .unwrap();
        // (e) a symlink escaping the root: a subdirectory link pointing at an OUTSIDE tree, whose
        // file must never be reached through the link.
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("escaped.rs"), "fn out_of_root() {}\n").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("outsider")).unwrap();

        // Ingest at width 1 (scope is width-independent) and collect the FILE each emitted content
        // key names (`<prefix>/<file>@<hash>#<i>`).
        let mut files: BTreeSet<String> = BTreeSet::new();
        ingest_project_batched_paced(root.to_str().unwrap(), 1, |batch| {
            for (key, _ev) in batch {
                if let Some((_, rest)) = key.split_once('/') {
                    if let Some(file) = rest.split('@').next() {
                        files.insert(file.to_string());
                    }
                }
            }
        });

        // Every in-root source file is ingested.
        assert!(
            files.contains("keep.rs"),
            "the in-root source file must be ingested; got {files:?}"
        );
        // NONE of the excluded paths leak in - not the gitignored build dir, the VCS metadata, the
        // rigger runtime dir, nor anything reached by escaping the root through the symlink.
        for f in &files {
            assert!(
                !f.starts_with("build/")
                    && !f.starts_with(".git")
                    && !f.starts_with(".rigger")
                    && !f.starts_with("outsider"),
                "an excluded path leaked into the ingest: {f:?} (all ingested: {files:?})"
            );
        }
        // Concretely: the walk ingests EXACTLY the one in-root source file, nothing else.
        assert_eq!(
            files,
            BTreeSet::from(["keep.rs".to_string()]),
            "the walk ingests only the project's own in-root source; got {files:?}"
        );
    }
}

#[cfg(all(test, feature = "symbols"))]
mod scoped_reindex_tests {
    //! Tests for [`ingest_files_batched`] and [`graph_index_lag`] (spec 92, FRESH ON EVERY
    //! INTEGRATION): the scoped-reindex entry an integration's own graph freshening calls, and the
    //! sampled staleness check `rigger validate`'s graph index-lag advisory calls.

    use super::{
        graph_index_lag, graph_index_lag_sample, ingest_files_batched,
        ingest_project_batched_paced, META_REPLAY_KEY,
    };
    use crate::eventstore::Event;

    /// Record `files`' CURRENT generation into a fresh `prior` stream, exactly as
    /// `graph_index_lag_reports_a_changed_file_and_not_an_unchanged_one` seeds its own fixture -
    /// extracted here so the sample-wrapper tests below can build a "the graph just recorded
    /// this" baseline without repeating the stamping boilerplate.
    fn record_current_generation(root: &str, files: &[String]) -> Vec<Event> {
        let mut prior: Vec<Event> = Vec::new();
        let mut pos = 1u64;
        ingest_files_batched(root, files, |keyed| {
            for (key, ev) in keyed {
                let mut e = (*ev).clone().with_meta(META_REPLAY_KEY, key.as_str());
                e.position = pos;
                pos += 1;
                prior.push(e);
            }
        });
        prior
    }

    /// [`ingest_files_batched`] is bounded to exactly the NAMED files - an untouched sibling never
    /// reaches the sink, even though it is present and indexable. Mirrors
    /// `grounder::symbols::events::tests::file_batches_is_scoped_to_the_named_files_only` one layer
    /// up, through the KEYED sink the conductor actually calls.
    #[test]
    fn ingest_files_batched_is_bounded_to_the_named_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn foo() {}\n").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn bar() {}\n").unwrap();
        let root = dir.path().to_str().unwrap();

        let mut seen_files: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let stats = ingest_files_batched(root, &["a.rs".to_string()], |keyed| {
            for (key, _) in keyed {
                seen_files.insert(key.clone());
            }
        });
        assert_eq!(
            stats.batches_emitted, 1,
            "exactly the one named file's batch"
        );
        assert!(
            seen_files.iter().all(|k| k.starts_with("gc/a.rs@")),
            "only a.rs's keys reach the sink, never b.rs's; got {seen_files:?}"
        );
        assert!(
            !seen_files.is_empty(),
            "the named file's real definition must key at least one event"
        );
    }

    /// Item I (AN INTEGRATION'S INGEST CARRIES THE DESIGN HALF): a NAMED design doc lowers into
    /// exactly the `gd/` batch the whole-project walk gives it, while an unnamed design doc and a
    /// named doc the walk scope excludes (rigger's own `.rigger` runtime dir) lower into none.
    #[test]
    fn ingest_files_batched_lowers_a_named_design_doc() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("docs")).unwrap();
        std::fs::create_dir_all(dir.path().join(".rigger")).unwrap();
        let doc = "# Architecture\n\n## The store\n\nThe `src/store.rs` module owns the log.\n";
        std::fs::write(dir.path().join("docs/architecture.md"), doc).unwrap();
        std::fs::write(dir.path().join("docs/unnamed.md"), doc).unwrap();
        std::fs::write(dir.path().join(".rigger/persona.md"), doc).unwrap();
        let root = dir.path().to_str().unwrap();
        let mut walked: Vec<String> = Vec::new();
        ingest_project_batched_paced(root, 1, |keyed| {
            walked.extend(keyed.iter().map(|(k, _)| k.clone()))
        });
        let mut named: Vec<String> = Vec::new();
        ingest_files_batched(
            root,
            &["docs/architecture.md".into(), ".rigger/persona.md".into()],
            |keyed| named.extend(keyed.iter().map(|(k, _)| k.clone())),
        );
        let design = |keys: &[String], prefix: &str| -> Vec<String> {
            keys.iter()
                .filter(|k| k.starts_with(prefix))
                .cloned()
                .collect()
        };

        assert!(
            !design(&walked, "gd/docs/architecture.md@").is_empty()
                && !design(&walked, "gd/docs/unnamed.md@").is_empty()
                && design(&walked, "gd/.rigger/").is_empty(),
            "premise: the whole walk lowers both docs and nothing under .rigger; got {walked:?}"
        );
        assert_eq!(
            design(&named, "gd/"),
            design(&walked, "gd/docs/architecture.md@"),
            "the named design doc lowers into the whole walk's own batch, and the unnamed and the \
             excluded docs into none"
        );
    }

    /// [`graph_index_lag`] finds a file the graph's own recorded generation no longer matches, and
    /// leaves an unchanged sibling alone - the core "the graph agrees with the tree, or it does not"
    /// comparison the validate advisory reports from. The unchanged sibling carries a `WHY:`
    /// rationale, so it has a design batch beside its code batch: the lag reads the code
    /// generation alone.
    #[test]
    fn graph_index_lag_reports_a_changed_file_and_not_an_unchanged_one() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("stable.rs"),
            "fn stable() {}\n// WHY: the unchanged sibling carries design intent too\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("churn.rs"), "fn original() {}\n").unwrap();
        let root = dir.path().to_str().unwrap();
        let files = vec!["stable.rs".to_string(), "churn.rs".to_string()];

        // Simulate what the graph has already recorded: both files' CURRENT (pre-edit) generation,
        // stamped with real replay keys exactly as `RunCtx::emit_keyed_batch` would.
        let mut prior: Vec<Event> = Vec::new();
        let mut pos = 1u64;
        ingest_files_batched(root, &files, |keyed| {
            for (key, ev) in keyed {
                let mut e = (*ev).clone().with_meta(META_REPLAY_KEY, key.as_str());
                e.position = pos;
                pos += 1;
                prior.push(e);
            }
        });
        assert!(
            !prior.is_empty(),
            "fixture precondition: both files must key at least one recorded event"
        );

        // The graph is fresh for both files right now - zero lag before anything changes.
        assert_eq!(
            graph_index_lag(root, &prior, &files),
            Vec::<String>::new(),
            "a graph that just recorded both files' current generation has zero lag"
        );

        // Edit ONLY churn.rs on disk; stable.rs is byte-identical to what the graph recorded.
        std::fs::write(dir.path().join("churn.rs"), "fn renamed() {}\n").unwrap();

        let lagging = graph_index_lag(root, &prior, &files);
        assert_eq!(
            lagging,
            vec!["churn.rs".to_string()],
            "only the file that actually changed since the graph's recording is reported as \
             lagging; stable.rs must not be, and churn.rs must be; got {lagging:?}"
        );
    }

    /// A file the graph has NEVER recorded (an empty `prior`) counts as lagging when it genuinely
    /// extracts to something - the "added since the graph was last built" shape.
    #[test]
    fn graph_index_lag_reports_a_file_the_graph_never_recorded() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("new.rs"), "fn brand_new() {}\n").unwrap();
        let root = dir.path().to_str().unwrap();

        let lagging = graph_index_lag(root, &[], &["new.rs".to_string()]);
        assert_eq!(
            lagging,
            vec!["new.rs".to_string()],
            "a file the graph has never recorded, and which genuinely extracts to something, is \
             lagging"
        );
    }

    /// [`graph_index_lag_sample`] derives ITS OWN candidate list from `prior` (spec 92, `rigger
    /// validate`'s graph index-lag advisory) rather than taking a caller-supplied file list: a
    /// file the graph has recorded (`a.rs`, unchanged) is checked, a file the graph has NEVER
    /// recorded (`brand_new.rs`, present on disk but outside `prior`) is left OUT of the
    /// candidate set entirely - that is coverage's question (criterion 2), never double-counted
    /// as THIS advisory's lag - and a file the graph recorded that no longer exists on disk
    /// (`deleted.rs`) is likewise left out, since re-checking it needs no bounded sample (an
    /// integration's own reindex retires it directly, Design/Constraints Walk).
    #[test]
    fn graph_index_lag_sample_derives_its_candidates_from_what_the_graph_has_recorded_and_still_exists(
    ) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn one() {}\n").unwrap();
        std::fs::write(dir.path().join("deleted.rs"), "fn gone() {}\n").unwrap();
        let recorded = vec!["a.rs".to_string(), "deleted.rs".to_string()];
        let prior = record_current_generation(root, &recorded);

        // deleted.rs no longer exists on disk; brand_new.rs exists but the graph never recorded
        // it (it is not in `prior` at all).
        std::fs::remove_file(dir.path().join("deleted.rs")).unwrap();
        std::fs::write(dir.path().join("brand_new.rs"), "fn brand_new() {}\n").unwrap();

        assert_eq!(
            graph_index_lag_sample(root, &prior),
            Vec::<String>::new(),
            "a.rs is unchanged since it was recorded, deleted.rs no longer exists (out of \
             scope), and brand_new.rs was never recorded (coverage's question, not this \
             advisory's) - zero lag"
        );
    }

    /// [`graph_index_lag_sample`] surfaces a genuine disagreement end to end: a file the graph
    /// recorded, still present on disk, whose content has since changed, is reported - the exact
    /// shape `rigger validate`'s advisory warns an operator about.
    #[test]
    fn graph_index_lag_sample_reports_a_file_that_changed_since_the_graph_recorded_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        std::fs::write(dir.path().join("churn.rs"), "fn original() {}\n").unwrap();
        let prior = record_current_generation(root, &["churn.rs".to_string()]);

        std::fs::write(dir.path().join("churn.rs"), "fn renamed() {}\n").unwrap();

        assert_eq!(
            graph_index_lag_sample(root, &prior),
            vec!["churn.rs".to_string()],
            "churn.rs disagrees with the graph's last recorded generation"
        );
    }

    /// The sample is BOUNDED (spec 92, cost-bounded like `grounder::symbols::staleness`'s own
    /// sample): recording more files than the sample size all still agreeing must still read as
    /// zero lag - the bound never manufactures a false positive by skipping a file.
    #[test]
    fn graph_index_lag_sample_is_bounded_and_stays_silent_when_every_sampled_file_agrees() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let files: Vec<String> = (0..20)
            .map(|i| {
                let name = format!("f{i}.rs");
                std::fs::write(dir.path().join(&name), format!("fn f{i}() {{}}\n")).unwrap();
                name
            })
            .collect();
        let prior = record_current_generation(root, &files);

        assert_eq!(
            graph_index_lag_sample(root, &prior),
            Vec::<String>::new(),
            "twenty unchanged recorded files, all agreeing, must read as zero lag regardless of \
             the sample bound"
        );
    }
}
