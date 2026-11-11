//! The project-source walk and its content key: the per-file extraction batches of the tree,
//! each keyed `<prefix>/<file>@<hash>#<i>`, which re-extraction and the offline graph passes
//! read. The log records none of it - the code store holds the code facts - and the folding
//! store that moves run events into the graph lives here beside it.
//!
//! Symbols-gated: the walk lowers the tree through the `symbols` extraction pass, so the light
//! lane has nothing to ingest - a no-op that emits nothing, exactly as the run's ingest is a
//! no-op there.

use crate::contextgraph::{fold_loss_clause, wired, Fold, RecordFold};
use crate::eventstore::{
    Appended, Error, Event, EventBatchSink, EventStore, ExpectedRevision, Filter, GroupHead,
    Position, Revision, Subscription, TypeSelection,
};

pub use rigger_domain::ingest::*;

/// Exactly the events `appended` reports the store placed, each stamped with the position the
/// store issued for it (see [`FoldingStore::append_and_fold`]) and with `stream`, the stream it
/// was appended to, so a fold of the copy reads it as the log does (spec 124, THE APPENDER):
/// what a fold of that append folds.
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
            events
                .get(i)
                .map(|e| e.clone().appended_at(stream, position))
        })
        .collect())
}

/// An [`EventStore`] that folds every event appended through it into the context graph, so a writer
/// that knows only the store port - a run's mint, a parked spawn, a recorded liveness fault, an
/// operator's resume - still leaves no event the graph's applied ledger misses. The append keeps
/// the caller's own expectation and goes to the log FIRST; only then is the graph opened, through
/// the `graph` opener the store was wired with (spec 101: a verb whose job is to append never opens
/// `graph.db` before its append), and the append folded at the positions the store issued. A fold
/// it could not make - the graph could not be had, owes its rebuild, or did not fold the events
/// inside the appender's ceiling (spec 124) - is said through `log`, never swallowed: the events
/// are on the log whatever became of the fold, and their positions missing from the graph's
/// `applied` ledger are what the next fold through a folding store reads. Wired
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
    graph: Option<&'a dyn RecordFold>,
    log: &'a (dyn Fn(&str) + Sync),
) -> FoldingStore<
    'a,
    impl Fn() -> Result<&'a dyn RecordFold, crate::contextgraph::Error> + Send + Sync + 'a,
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
    G: std::ops::Deref<Target = dyn RecordFold + 'g>,
{
    /// THE APPEND-THEN-FOLD BODY of every event but a ledger entry: append `events` to `stream` under the caller's `expected`
    /// revision in ONE store append, then fold exactly what the store placed, at the positions it
    /// issued, through ONE [`Fold::of_batch`], at the transaction cadence
    /// [`RecordFold::apply_batch`] states - the batched fold spec 49 needs, rather than one fold
    /// per event. The append goes to the log first and the graph is opened only after it (spec
    /// 101: a verb whose job is to append never opens
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
    /// Every batched append-then-fold is this body - every run event through
    /// [`EventStore::append`] below, the offline graph passes and `rigger reset --runs` through
    /// this method - so the batching and the fold can never diverge between them. Two
    /// single-event folds are not this body: `rigger emit`
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

impl<O> FoldingStore<'_, O> {
    /// Say through the injected log that `recorded` event(s) are on the log and `fold`, the fold
    /// of them into the wired graph, was not made, and why: the one spelling of a lost fold every
    /// append through this store is said in. A fold that was made has nothing to say, and neither
    /// has a store wired with no graph, which folds nothing by design.
    pub fn say_fold_lost(&self, recorded: usize, fold: &Fold) {
        if self.graph.is_some() && *fold != Fold::Folded {
            (self.log)(&format!(
                "rigger: recorded {recorded} run event(s){}",
                fold_loss_clause(fold)
            ));
        }
    }
}

impl<'g, O, G> EventStore for FoldingStore<'_, O>
where
    O: Fn() -> Result<G, crate::contextgraph::Error> + Send + Sync,
    G: std::ops::Deref<Target = dyn RecordFold + 'g>,
{
    fn append(
        &self,
        stream: &str,
        expected: ExpectedRevision,
        events: &[Event],
    ) -> Result<Appended, Error> {
        let done = self.append_and_fold(stream, expected, events)?;
        self.say_fold_lost(events.len(), &done.fold);
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

    fn read_all_page(
        &self,
        from: Position,
        filter: &Filter,
        max: usize,
    ) -> Result<Vec<Event>, Error> {
        self.store.read_all_page(from, filter, max)
    }

    fn log_head(&self) -> Result<Option<Position>, Error> {
        self.store.log_head()
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

/// Walk the project tree at `root` and hand `on_batch` each file's WHOLE keyed batch, with its
/// flag (spec 107), of the
/// extraction events the code (spec 29a), design (spec 29b), and workflow-definition (spec 92
/// criterion 2) passes emit: the file's events, each paired with its deterministic content key
/// `<prefix>/<file>@<hash>#<i>` (`gc` for code, `gd` for design, `gw` for the workflow
/// definition), in `#i` order. The key is a pure function
/// of the batch's bytes ALONE, so the same content always yields the same keys and different
/// content always yields different ones. A key is therefore a CONTENT GENERATION of a file, not a
/// mark that the file has been seen: whether a batch is to be recorded is a question about the
/// file's LATEST recorded generation, never about whether the generation was recorded at any
/// time, which is why a file reverted to content it held earlier is recorded again. This function
/// owns only the walk and the keying; the sink decides what to do with a batch.
/// The flag beside a batch says whether the walk excluded its identity as an out-of-line
/// test module's: set for the `gc` batch of a file [`walk_exclusions`] names and clear for every
/// other batch, so a sink never computes it or loads an index.
///
/// "Content" here is the batch this walk LOWERED, which is not always the file on disk, and the two
/// halves differ: the design half reads the live tree, while the code half derives its events
/// from the symbols a fresh build of the tree indexed (see [`walk_batches`]), so a file edited
/// between that build and the sink's own read lowers to bytes the sink does not record. A file the
/// walk no longer sees is never lowered: every identity of `held` - the identities the graph holds a
/// generation for ([`RecordFold::held_identities`]) - whose file the walk no longer sees is handed
/// the batch a sink extracts from no bytes, so its fold retires what the file asserted (see
/// [`walk_batches`]).
///
/// The code half's per-file parse/lower fans across a default-sized worker pool (one worker per
/// logical core), but the EMIT stays in sorted file-path order - parallelism is observationally
/// invisible. The returned [`IngestStats`] is informational.
#[cfg(feature = "symbols")]
pub fn ingest_project_batched(
    root: &str,
    held: &[String],
    on_batch: impl BatchSink,
) -> IngestStats {
    ingest_project_batched_paced(root, crate::parallel::default_workers(), held, on_batch)
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
    held: &[String],
    on_batch: impl BatchSink,
) -> IngestStats {
    walk_batches(root, workers, held, on_batch)
}

/// The walk behind [`ingest_project_batched_paced`]: parse/lower the project at `root` and hand
/// each file's WHOLE keyed batch to `on_batch`, in sorted file-path order (the code half first,
/// then the design half, then the workflow-definition half), each batch in `#i` order, then the
/// retirement batch of each identity of `held` whose file the walk no longer sees, in `held`'s
/// order. A code batch is handed with the flag the code half
/// answers beside it, every other batch with the flag clear.
///
/// The code half lowers only the indexed files the walk sees ([`walk_sees`]), so an index that
/// still lists a deleted file never hands that file's old batch. A retirement
/// batch is the batch a sink extracts from no bytes under the identity's prefix ([`extraction`]),
/// keyed as every batch is: a sink records it once, as a new generation of the identity whose
/// fold retires every node the file's prior generation asserted, and a later walk hands the
/// generation the log and the graph both hold, which records nothing. Which identities retire is
/// read from `held` and the tree alone, never from the symbols index.
#[cfg(feature = "symbols")]
fn walk_batches(
    root: &str,
    workers: usize,
    held: &[String],
    mut on_batch: impl BatchSink,
) -> IngestStats {
    let mut batches_emitted = 0usize;
    let walked = crate::grounder::walked_paths(std::path::Path::new(root));
    let sees = |prefix: &str, path: &str| walk_sees(root, &walked, prefix, path);
    // The code half (spec 29a): parallel parse feeds this ordered emit.
    let (code_batches, workers_engaged) =
        crate::grounder::symbols::events::project_batches_paced(root, workers);
    for (file, batch, excluded) in code_batches.iter().filter(|(file, ..)| sees("gc", file)) {
        key_batch("gc", file, batch, *excluded, &mut on_batch);
        batches_emitted += 1;
    }
    // The design half (spec 29b): the project's design docs and inline source rationale, serial.
    let design_batches = crate::grounder::design::events::project_batches(root);
    for (file, batch) in &design_batches {
        key_batch("gd", file, batch, false, &mut on_batch);
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
        key_batch("gw", file, batch, false, &mut on_batch);
        batches_emitted += 1;
    }
    for (prefix, path, half) in retired_identities(held, sees) {
        key_batch(
            prefix,
            path,
            &extraction(half, path, None, false),
            false,
            &mut on_batch,
        );
        batches_emitted += 1;
    }
    IngestStats {
        batches_emitted,
        workers_engaged,
    }
}

/// Whether the walk under `root` sees the file at `path` under `prefix`: one `walked` holds
/// ([`crate::grounder::walked_paths`]), or one the tree's read rule admits
/// ([`crate::grounder::tree_holds`]), which is asked only of a path `walked` lacks - the workflow
/// definition's included, since it sits under a hidden directory the walk never enters.
#[cfg(feature = "symbols")]
fn walk_sees(
    root: &str,
    walked: &std::collections::BTreeSet<String>,
    prefix: &str,
    path: &str,
) -> bool {
    walked.contains(path) || crate::grounder::tree_holds(std::path::Path::new(root), prefix, path)
}

/// The identities [`walk_batches`] retires (spec 107): each identity of `held`, in its order,
/// whose prefix names a half and whose file the walk does not see (`sees`), as its prefix, path
/// and half.
#[cfg(feature = "symbols")]
fn retired_identities(
    held: &[String],
    sees: impl Fn(&str, &str) -> bool,
) -> Vec<(&str, &str, Half)> {
    held.iter()
        .filter_map(|identity| {
            let (prefix, path) =
                rigger_domain::retention::GenerationIngested::identity_parts(identity)?;
            let half = half_of(prefix)?;
            (!sees(prefix, path)).then_some((prefix, path, half))
        })
        .collect()
}

/// [`ingest_project_batched`] SCOPED to exactly the NAMED `files` (spec 92, FRESH ON EVERY
/// INTEGRATION), rather than walking the whole project: the property an integration's OWN reindex
/// needs, bounded by the merge's OWN file list (Design/Constraints Walk: "the reindex is bounded by
/// the merge's file list"), never the project's total file count. Reuses the SAME per-file lowering
/// and keying as the whole-project walk (`crate::grounder::symbols::events::file_batches` for the
/// code half, `crate::grounder::design::events::named_batches` for the design half, and
/// [`key_batch`], the identical authorities [`walk_batches`] calls) - never a second lowering path -
/// so a named file's scoped batch is byte-identical to what a full walk would produce for it, and
/// the generation a batch is recorded under can never drift between the two entries.
///
/// Both halves: the code (`gc/`) batch of every named file, then the design-intent (`gd/`, spec
/// 29b) batch of every named file the walk scope admits that carries design intent - so a doc an
/// integration edits or regenerates reaches the graph as its code does. Each code batch is handed
/// with the flag the code half answers beside it, each design batch with the flag clear, as the
/// whole-project walk hands them.
#[cfg(feature = "symbols")]
pub fn ingest_files_batched(
    root: &str,
    files: &[String],
    mut on_batch: impl BatchSink,
) -> IngestStats {
    let code_batches = crate::grounder::symbols::events::file_batches(root, files);
    let design_batches = crate::grounder::design::events::named_batches(root, files);
    let mut batches_emitted = 0usize;
    for (file, batch, excluded) in &code_batches {
        key_batch("gc", file, batch, *excluded, &mut on_batch);
        batches_emitted += 1;
    }
    for (file, batch) in &design_batches {
        key_batch("gd", file, batch, false, &mut on_batch);
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

/// The `gc` identity of `path`: the identity the walk keys the file's code batch under, and the
/// one [`walk_exclusions`] names an excluded file by.
#[cfg(feature = "symbols")]
pub(crate) fn code_identity(path: &str) -> String {
    format!("gc/{path}")
}

/// The index the walk's code half lowers and the identities it excludes (spec 107): a fresh
/// build of the tree, in memory only and never persisted - and the `gc/<path>`
/// identity of every file that index resolves as an out-of-line test module's target
/// ([`out_of_line_test_module_files`](crate::grounder::symbols::events::out_of_line_test_module_files)).
/// Only `gc` identities are named, so the same path's batch under another prefix is never excluded.
#[cfg(feature = "symbols")]
pub fn walk_exclusions(
    root: &str,
) -> (
    crate::grounder::symbols::model::SymbolIndex,
    std::collections::BTreeSet<String>,
) {
    let index = crate::grounder::symbols::build_index(root, None);
    let excluded = crate::grounder::symbols::events::out_of_line_test_module_files(&index)
        .iter()
        .map(|path| code_identity(path))
        .collect();
    (index, excluded)
}

/// Light lane: no extraction pass is compiled, so there is no index to load and no file to
/// exclude - the empty index and the empty set.
#[cfg(not(feature = "symbols"))]
pub fn walk_exclusions(
    _root: &str,
) -> (
    crate::grounder::symbols::model::SymbolIndex,
    std::collections::BTreeSet<String>,
) {
    (
        crate::grounder::symbols::model::SymbolIndex::default(),
        std::collections::BTreeSet::new(),
    )
}

/// The generation of an unkeyed `batch` (spec 107): the content hash of its events' bytes
/// concatenated in order, by the line-ending-normalized content primitive
/// ([`crate::grounder::symbols::content_hash`], so the change-detection key is one
/// content-identity authority). The batch bytes are JSON the emit pass just serialized, so they
/// are valid UTF-8.
#[cfg(feature = "symbols")]
pub fn batch_generation(batch: &[Event]) -> String {
    let concat: String = batch
        .iter()
        .filter_map(|e| std::str::from_utf8(&e.data).ok())
        .collect();
    crate::grounder::symbols::content_hash(&concat)
}

/// A half's `(path, bytes, excluded)` function (spec 107): the batch the half extracts from the
/// `bytes` of the file at `path` under the walk's flag.
#[cfg(feature = "symbols")]
type Half = fn(&str, Option<&[u8]>, bool) -> Vec<Event>;

/// The `(path, bytes, excluded)` function of the half that extracts batches under `prefix` (spec
/// 107): `gc` the code half's, `gd` the design half's, `gw` the workflow definition's, and none
/// for a prefix that names no half.
#[cfg(feature = "symbols")]
fn half_of(prefix: &str) -> Option<Half> {
    match prefix {
        "gc" => Some(crate::grounder::symbols::events::bytes_batch),
        "gd" => Some(crate::grounder::design::events::bytes_batch),
        "gw" => Some(crate::grounder::workflowdef::bytes_batch),
        _ => None,
    }
}

/// The batch `half` extracts from the `bytes` of the file at `path` under the walk's flag
/// `excluded`, as every sink and every resolution reads it (spec 107): the half's own batch for
/// bytes, and for no bytes the one boundary event of a path that holds nothing
/// ([`unparsed_batch`](crate::grounder::symbols::events::unparsed_batch)) whatever the half, so
/// a file gone from the tree has a generation under every prefix, and its fold retires what the
/// file asserted.
#[cfg(feature = "symbols")]
fn extraction(half: Half, path: &str, bytes: Option<&[u8]>, excluded: bool) -> Vec<Event> {
    match bytes {
        Some(_) => half(path, bytes, excluded),
        None => crate::grounder::symbols::events::unparsed_batch(path),
    }
}

/// Whether the next ingest of the file the identity `<prefix>/<path>` names records an entry for
/// it (spec 107): whether the tree's one read rule ([`crate::grounder::tree_bytes`]) hands bytes
/// for its path under `root` that its half extracts a batch from ([`extracts`]). An identity
/// whose path holds no such file - gone, outside the walk's scope, unreadable - answers false:
/// what the walk hands it is its retirement batch from no bytes ([`ingest_project_batched`]),
/// which retires the file rather than bringing a recording of it up to date, so it is not
/// counted here. One the domain cuts into no prefix and path
/// ([`rigger_domain::retention::GenerationIngested::identity_parts`]) names no file.
pub fn next_ingest_records(root: &std::path::Path, identity: &str) -> bool {
    rigger_domain::retention::GenerationIngested::identity_parts(identity).is_some_and(
        |(prefix, path)| {
            crate::grounder::tree_bytes(root, prefix, path)
                .is_some_and(|bytes| extracts(prefix, path, &bytes))
        },
    )
}

/// Whether the half under `prefix` maps the `bytes` of the file at `path` to a batch that is not
/// empty: every `gc` input is, whatever the walk's flag, which no other half reads. A prefix that
/// names no half extracts nothing.
#[cfg(feature = "symbols")]
fn extracts(prefix: &str, path: &str, bytes: &[u8]) -> bool {
    half_of(prefix).is_some_and(|half| !half(path, Some(bytes), false).is_empty())
}

/// Light lane: no extraction pass is compiled, so no half is asked and the file the tree holds
/// stands for its batch (mirrors [`resolve_entry`]'s own light-lane stub).
#[cfg(not(feature = "symbols"))]
fn extracts(_prefix: &str, _path: &str, _bytes: &[u8]) -> bool {
    true
}

/// Where [`resolve_entry`] reads an entry's blob (spec 107): an object id to the bytes the
/// repository's object database holds under it, none when it holds none, or the failure of the
/// read, which fails the resolution.
pub type BlobSource<'b> =
    dyn FnMut(&str) -> Result<Option<Vec<u8>>, crate::contextgraph::Error> + 'b;

/// Re-extract the batch of the ledger entry `entry`, whose path is relative to `root` (spec 107,
/// RESOLUTION IS BY GENERATION): the batch its half's `(path, bytes, excluded)` function answers,
/// under the entry's recorded flag, for the first source whose batch has the entry's generation
/// ([`batch_generation`]) - (1) the entry's blob, when the entry names one and `blobs` holds it,
/// `blobs` being absent when no object database can be asked; (2) the tree's file at the path,
/// as the tree's one read rule hands it ([`crate::grounder::tree_bytes`]); (3) no bytes. The blob
/// is where to look first, never the test. It answers none when no source resolves the entry:
/// when none extracts to its generation, when its prefix names no half, and when its payload
/// does not parse. An empty batch has no generation, so it resolves no entry. A blob source
/// that fails fails the resolution.
#[cfg(feature = "symbols")]
pub fn resolve_entry(
    root: &std::path::Path,
    entry: &Event,
    blobs: Option<&mut BlobSource>,
) -> Result<Option<Vec<Event>>, crate::contextgraph::Error> {
    let Ok(named) = rigger_domain::retention::GenerationIngested::parse(&entry.data) else {
        return Ok(None);
    };
    let Some(half) = half_of(&named.prefix) else {
        return Ok(None);
    };
    let resolved = |bytes: Option<Vec<u8>>| {
        let batch = extraction(half, &named.file, bytes.as_deref(), named.excluded);
        (!batch.is_empty() && batch_generation(&batch) == named.generation).then_some(batch)
    };
    let blob = match blobs {
        Some(blobs) if !named.blob.is_empty() => blobs(&named.blob)?,
        _ => None,
    };
    let from_held = |held: Option<Vec<u8>>| held.and_then(|bytes| resolved(Some(bytes)));
    Ok(from_held(blob)
        .or_else(|| {
            from_held(crate::grounder::tree_bytes(
                root,
                &named.prefix,
                &named.file,
            ))
        })
        .or_else(|| resolved(None)))
}

/// Light lane: no extraction pass is compiled, so no source re-extracts a batch - every entry is
/// unresolved, and the blob source is never asked (mirrors [`walk_exclusions`]'s own light-lane
/// stub).
#[cfg(not(feature = "symbols"))]
pub fn resolve_entry(
    _root: &std::path::Path,
    _entry: &Event,
    _blobs: Option<&mut BlobSource>,
) -> Result<Option<Vec<Event>>, crate::contextgraph::Error> {
    Ok(None)
}

/// Key one file's batch under `<prefix>/<file>@<hash>#<i>` and hand the WHOLE keyed batch to
/// `on_batch` at once, with `excluded`, the batch's flag, as the walk answered it. `hash` is the
/// batch's generation ([`batch_generation`]), so every event of a file shares one `<hash>`, and
/// the flag changes no key.
#[cfg(feature = "symbols")]
fn key_batch(
    prefix: &str,
    file: &str,
    batch: &[Event],
    excluded: bool,
    on_batch: &mut impl BatchSink,
) {
    let hash = batch_generation(batch);
    let keyed: Vec<(String, &Event)> = batch
        .iter()
        .enumerate()
        .map(|(i, ev)| (format!("{prefix}/{file}@{hash}#{i}"), ev))
        .collect();
    on_batch(&keyed, excluded);
}

/// The light lane compiles no extraction pass, so there is nothing to walk - a no-op that hands the
/// sink no batches. `graph build` still opens (creating) the store and degrades to an empty graph,
/// never an error, in either lane (the batched append-and-fold kernel above stays compiled in both).
#[cfg(not(feature = "symbols"))]
pub fn ingest_project_batched(_root: &str, _held: &[String], _on_batch: impl BatchSink) {}

#[cfg(all(test, feature = "symbols"))]
mod tests {
    use super::{ingest_project_batched_paced, IngestStats};
    use crate::extraction_tree::planted_extraction_tree;

    /// Drive a walk at `workers` width and capture the exact `(key, type, data)` triples the sink
    /// sees, in emit order - the observable the byte-identical contract is defined over.
    fn walk(root: &str, workers: usize) -> (Vec<(String, String, Vec<u8>)>, IngestStats) {
        let mut seq: Vec<(String, String, Vec<u8>)> = Vec::new();
        let stats = ingest_project_batched_paced(root, workers, &[], |batch, _| {
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
        ingest_project_batched_paced(root.to_str().unwrap(), 1, &[], |batch, _| {
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

    /// The paths an index holds, in its own sorted order.
    fn indexed_paths(index: &crate::grounder::symbols::model::SymbolIndex) -> Vec<&str> {
        index.files().keys().map(String::as_str).collect()
    }

    /// THE EXTRACTION READS BYTES (spec 107): over the extraction tree, whose out-of-line test
    /// module also has a `gd` batch, `walk_exclusions` names that module's `gc` identity and
    /// nothing else, beside the index it read the tree into - built in memory, never persisted.
    #[test]
    fn walk_exclusions_names_the_out_of_line_test_modules_gc_identity_and_no_other() {
        use crate::extraction_tree::{SOURCE_PATH, TEST_MODULE_PATH};

        let dir = planted_extraction_tree(crate::host_fixtures::write_file);
        let root = dir.path().to_str().unwrap();

        let (index, excluded) = super::walk_exclusions(root);
        assert_eq!(
            excluded,
            std::collections::BTreeSet::from(["gc/src/checks.rs".to_string()])
        );
        assert_eq!(indexed_paths(&index), vec![TEST_MODULE_PATH, SOURCE_PATH]);
    }

    /// `code_identity` answers a path's `gc` identity: the path whole, nested directories and
    /// all, under the `gc` prefix - the string `walk_exclusions` names an excluded file by.
    #[test]
    fn code_identity_answers_the_path_under_the_gc_prefix() {
        assert_eq!(
            super::code_identity("src/deep/checks.rs"),
            "gc/src/deep/checks.rs"
        );
        assert_eq!(super::code_identity("lib.rs"), "gc/lib.rs");
    }

    /// The walk's keyed batches over the extraction tree: every batch in emit order, each event
    /// keyed `<prefix>/<path>@<generation>#<i>` with the type and bytes the walk lowers.
    #[test]
    fn the_walk_keys_the_extraction_trees_batches_as_recorded() {
        let dir = planted_extraction_tree(crate::host_fixtures::write_file);

        let (seq, stats) = walk(dir.path().to_str().unwrap(), 1);
        let expected: Vec<(String, String, Vec<u8>)> = crate::extraction_tree::WALKED
            .iter()
            .flat_map(|batch| {
                batch
                    .events
                    .iter()
                    .enumerate()
                    .map(move |(i, (type_, data))| {
                        (
                            format!("{}/{}@{}#{i}", batch.prefix, batch.path, batch.generation),
                            type_.to_string(),
                            data.as_bytes().to_vec(),
                        )
                    })
            })
            .collect();
        assert_eq!(seq, expected);
        assert_eq!(stats.batches_emitted, 6);
    }

    /// The identity and the flag of each batch `walk` hands the recording sink it is given, in
    /// the order the sink saw them.
    fn handed_flags(walk: impl FnOnce(&mut dyn super::BatchSink)) -> Vec<(String, bool)> {
        let mut handed: Vec<(String, bool)> = Vec::new();
        walk(
            &mut |batch: &[(String, &crate::eventstore::Event)], excluded: bool| {
                let identity = super::derived_key_parts(&batch[0].0).unwrap().0;
                handed.push((identity.to_string(), excluded));
            },
        );
        handed
    }

    /// THE WALK HANDS EACH BATCH WITH ITS FLAG (spec 107), at the whole-tree walk: over the
    /// extraction tree the out-of-line test module's `gc` batch is handed with its flag set and
    /// every other batch with it clear - the other `gc` file's, each `gd` batch, the same
    /// path's among them, and the `gw` batch.
    #[test]
    fn the_whole_tree_walk_hands_only_the_out_of_line_test_modules_gc_batch_with_its_flag_set() {
        let dir = planted_extraction_tree(crate::host_fixtures::write_file);
        let root = dir.path().to_str().unwrap();

        let handed = handed_flags(|sink| {
            ingest_project_batched_paced(root, 1, &[], sink);
        });
        assert_eq!(handed, crate::extraction_tree::walked_handoffs());
        let set: Vec<&str> = handed
            .iter()
            .filter(|(_, excluded)| *excluded)
            .map(|(identity, _)| identity.as_str())
            .collect();
        assert_eq!(set, ["gc/src/checks.rs"]);
        assert_eq!(handed.len(), 6);
    }

    /// THE WALK HANDS EACH BATCH WITH ITS FLAG (spec 107), at the integration reindex: of the
    /// named files' batches only the out-of-line test module's `gc` batch is handed with its
    /// flag set - the `gc` batch of a path the index lacks, of a document and of the other
    /// source file, and every `gd` batch, are handed with it clear.
    #[test]
    fn the_integration_reindex_hands_only_the_out_of_line_test_modules_gc_batch_with_its_flag_set()
    {
        use crate::extraction_tree::{DOCUMENT_PATH, SOURCE_PATH, TEST_MODULE_PATH};

        let dir = planted_extraction_tree(crate::host_fixtures::write_file);
        let root = dir.path().to_str().unwrap();
        let files = [
            SOURCE_PATH.to_string(),
            "src/absent.rs".to_string(),
            TEST_MODULE_PATH.to_string(),
            DOCUMENT_PATH.to_string(),
        ];

        let handed = handed_flags(|sink| {
            super::ingest_files_batched(root, &files, sink);
        });
        assert_eq!(
            handed,
            [
                ("gc/src/lib.rs".to_string(), false),
                ("gc/src/absent.rs".to_string(), false),
                ("gc/src/checks.rs".to_string(), true),
                ("gc/docs/architecture.md".to_string(), false),
                ("gd/docs/architecture.md".to_string(), false),
                ("gd/src/checks.rs".to_string(), false),
                ("gd/src/lib.rs".to_string(), false),
            ]
        );
    }

    /// `key_batch` hands the sink the flag it is handed, set or clear, beside the batch keyed
    /// as it keys it whatever the flag.
    #[test]
    fn key_batch_hands_the_sink_the_flag_it_is_handed_beside_the_keyed_batch() {
        use crate::eventstore::Event;

        let batch = [
            Event::new("CodeEntityExtracted", b"{\"name\":\"a\"}".to_vec()),
            Event::new("EdgeInferred", b"{\"name\":\"b\"}".to_vec()),
        ];
        let generation = super::batch_generation(&batch);
        for excluded in [true, false] {
            let mut keyed_events: Vec<(String, String, Vec<u8>)> = Vec::new();
            let mut flags: Vec<bool> = Vec::new();
            super::key_batch(
                "gc",
                "src/a.rs",
                &batch,
                excluded,
                &mut |keyed: &[(String, &Event)], flag: bool| {
                    for (key, event) in keyed {
                        keyed_events.push((key.clone(), event.type_.clone(), event.data.clone()));
                    }
                    flags.push(flag);
                },
            );
            assert_eq!(
                keyed_events,
                vec![
                    (
                        format!("gc/src/a.rs@{generation}#0"),
                        "CodeEntityExtracted".to_string(),
                        b"{\"name\":\"a\"}".to_vec()
                    ),
                    (
                        format!("gc/src/a.rs@{generation}#1"),
                        "EdgeInferred".to_string(),
                        b"{\"name\":\"b\"}".to_vec()
                    ),
                ],
                "key_batch keyed the batch under the flag {excluded}"
            );
            assert_eq!(
                flags,
                vec![excluded],
                "key_batch handed the whole batch once, with the flag {excluded}"
            );
        }
    }

    /// `batch_generation` answers the generation `key_batch` keys: for every batch the walk hands
    /// its sink over the extraction tree, the generation of the batch's events is the one the
    /// tree records and the one every key of the batch carries.
    #[test]
    fn batch_generation_answers_the_generation_key_batch_keys() {
        let dir = planted_extraction_tree(crate::host_fixtures::write_file);

        let mut generations: Vec<String> = Vec::new();
        let mut keys: Vec<String> = Vec::new();
        let mut expected_keys: Vec<String> = Vec::new();
        ingest_project_batched_paced(dir.path().to_str().unwrap(), 1, &[], |batch, _| {
            let events: Vec<crate::eventstore::Event> =
                batch.iter().map(|(_, event)| (*event).clone()).collect();
            let generation = super::batch_generation(&events);
            for (i, (key, _)) in batch.iter().enumerate() {
                let identity = super::derived_key_parts(key).unwrap().0;
                keys.push(key.clone());
                expected_keys.push(format!("{identity}@{generation}#{i}"));
            }
            generations.push(generation);
        });

        let recorded: Vec<&str> = crate::extraction_tree::WALKED
            .iter()
            .map(|batch| batch.generation)
            .collect();
        assert_eq!(generations, recorded);
        assert_eq!(keys, expected_keys);
    }

    /// `batch_generation` is the content hash of the batch's event bytes concatenated in order:
    /// FNV-1a over `{"a":1}{"b":2}`, another value for the reverse order, the offset basis for
    /// no events, and an event whose bytes are not UTF-8 contributing nothing.
    #[test]
    fn batch_generation_hashes_the_batchs_event_bytes_in_order() {
        use crate::eventstore::Event;

        let a = Event::new("A", br#"{"a":1}"#.to_vec());
        let b = Event::new("B", br#"{"b":2}"#.to_vec());
        let not_utf8 = Event::new("C", vec![0xff, 0xfe]);

        assert_eq!(
            super::batch_generation(&[a.clone(), b.clone()]),
            "bbc8136738fb1e2b"
        );
        assert_eq!(
            super::batch_generation(&[b.clone(), a.clone()]),
            "10432fcb82c8721f"
        );
        assert_eq!(
            super::batch_generation(std::slice::from_ref(&a)),
            "9c3e82dd6fcae8b1"
        );
        assert_eq!(super::batch_generation(&[]), "cbf29ce484222325");
        assert_eq!(
            super::batch_generation(&[a, not_utf8, b]),
            "bbc8136738fb1e2b"
        );
    }
}

#[cfg(all(test, feature = "symbols"))]
mod resolve_entry_tests {
    //! Tests for [`resolve_entry`] (spec 107, THE REBUILD RE-EXTRACTS THE LEDGER): resolution by
    //! generation from the blob, the tree's file and no bytes, in that order.

    use super::{batch_generation, resolve_entry};
    use crate::contextgraph::Error;
    use crate::eventstore::Event;
    use crate::extraction_tree::{
        planted_extraction_tree, walked_batch, walked_generation, DOCUMENT_PATH, SOURCE_BODY,
        SOURCE_PATH, TEST_MODULE_PATH, WORKFLOW_PATH,
    };
    use crate::host_fixtures::write_file;
    use crate::test_support::{events_of, generation_ingested, wire_owned};
    use rigger_domain::retention::TYPE_GENERATION_INGESTED;
    use std::path::Path;

    /// A source file the fixture does not hold, the `gc` batch it extracts to at `src/lib.rs`,
    /// and the `gc` batch of that path for no bytes.
    pub(super) const OTHER_BODY: &str = "fn tree_only() {}\n";
    pub(super) const OTHER_BATCH: [(&str, &str); 2] = [
        (
            "CodeEntityExtracted",
            r#"{"file":"src/lib.rs","name":"tree_only","kind":"function","line":1,"lang":"rust","fresh":true}"#,
        ),
        (
            "EdgeInferred",
            r#"{"file":"src/lib.rs","name":"","lang":"rust","fresh":true,"is_test":true}"#,
        ),
    ];
    pub(super) const NO_BYTES_BATCH: [(&str, &str); 1] = [(
        "EdgeInferred",
        r#"{"file":"src/lib.rs","name":"","lang":"unknown","fresh":true}"#,
    )];

    /// The generation of the batch holding `events`, `(type, payload text)` pairs.
    pub(super) fn generation_of(events: &[(&str, &str)]) -> String {
        batch_generation(&events_of(events))
    }

    /// The ledger entry of `<prefix>/<file>` at `generation`, extracted from `blob` under the
    /// walk's flag `excluded`.
    fn entry(prefix: &str, file: &str, generation: &str, blob: &str, excluded: bool) -> Event {
        generation_ingested(prefix, file, generation, blob, excluded).event(1)
    }

    /// Resolve `entry` under `root` against a blob source holding `held` - `(object id, bytes)`
    /// pairs - and answer the resolved batch as `(type, payload text)` pairs beside the object
    /// ids the source was asked for, in order.
    fn resolved(
        root: &Path,
        entry: &Event,
        held: &[(&str, &str)],
    ) -> (Option<Vec<(String, String)>>, Vec<String>) {
        let mut asked: Vec<String> = Vec::new();
        let mut source = |id: &str| -> Result<Option<Vec<u8>>, Error> {
            asked.push(id.to_string());
            Ok(held
                .iter()
                .find(|(held_id, _)| *held_id == id)
                .map(|(_, bytes)| bytes.as_bytes().to_vec()))
        };
        let batch = resolve_entry(root, entry, Some(&mut source)).unwrap();
        (batch.as_deref().map(wire_owned), asked)
    }

    /// `events`, borrowed `(type, payload text)` pairs, as owned ones.
    pub(super) fn pairs(events: &[(&str, &str)]) -> Vec<(String, String)> {
        wire_owned(&events_of(events))
    }

    /// A tree holding [`OTHER_BODY`] at the source file's path.
    fn tree_holding_the_other_body() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join(SOURCE_PATH), OTHER_BODY.as_bytes());
        dir
    }

    /// The blob is where to look first, the tree's file second and no bytes last: with the blob
    /// holding one generation and the tree's file another, an entry of each resolves to its own
    /// batch, an entry of the generation of no bytes resolves to that boundary, and an entry of a
    /// generation none of the three extracts to resolves nothing. The blob is asked for every
    /// one of them.
    #[test]
    fn an_entry_resolves_at_the_first_source_whose_batch_has_its_generation() {
        let dir = tree_holding_the_other_body();
        let held = [("b1", SOURCE_BODY)];
        let cases = [
            (
                walked_generation("gc", SOURCE_PATH).to_string(),
                Some(pairs(walked_batch("gc", SOURCE_PATH))),
            ),
            (generation_of(&OTHER_BATCH), Some(pairs(&OTHER_BATCH))),
            (generation_of(&NO_BYTES_BATCH), Some(pairs(&NO_BYTES_BATCH))),
            ("0000000000000000".to_string(), None),
        ];
        for (generation, expected) in cases {
            assert_eq!(
                resolved(
                    dir.path(),
                    &entry("gc", SOURCE_PATH, &generation, "b1", false),
                    &held
                ),
                (expected, vec!["b1".to_string()]),
                "the entry at {generation}"
            );
        }
    }

    /// An entry that names no blob never asks the blob source, and one whose blob the source
    /// does not hold falls to the tree's file.
    #[test]
    fn an_entry_naming_no_blob_asks_no_source_and_a_blob_not_held_falls_to_the_tree() {
        let dir = tree_holding_the_other_body();
        let other = generation_of(&OTHER_BATCH);
        assert_eq!(
            resolved(
                dir.path(),
                &entry("gc", SOURCE_PATH, &other, "", false),
                &[("", SOURCE_BODY)]
            ),
            (Some(pairs(&OTHER_BATCH)), Vec::<String>::new())
        );
        assert_eq!(
            resolved(
                dir.path(),
                &entry("gc", SOURCE_PATH, &other, "b2", false),
                &[("b1", SOURCE_BODY)]
            ),
            (Some(pairs(&OTHER_BATCH)), vec!["b2".to_string()])
        );
    }

    /// With no blob source, the first source is skipped: an entry whose generation only its blob
    /// extracts to resolves nothing, and one the tree's file extracts to resolves from it.
    #[test]
    fn without_a_blob_source_an_entry_resolves_from_the_tree_or_not_at_all() {
        let dir = tree_holding_the_other_body();
        let resolve = |generation: &str| {
            resolve_entry(
                dir.path(),
                &entry("gc", SOURCE_PATH, generation, "b1", false),
                None,
            )
            .unwrap()
            .as_deref()
            .map(wire_owned)
        };
        assert_eq!(resolve(walked_generation("gc", SOURCE_PATH)), None);
        assert_eq!(
            resolve(&generation_of(&OTHER_BATCH)),
            Some(pairs(&OTHER_BATCH))
        );
    }

    /// A blob source that fails fails the resolution with its error, though the tree's file
    /// extracts to the entry's generation.
    #[test]
    fn a_failing_blob_source_fails_the_resolution_the_tree_could_have_answered() {
        let dir = tree_holding_the_other_body();
        let mut failing =
            |_: &str| -> Result<Option<Vec<u8>>, Error> { Err(Error("the batch died".into())) };
        let answer = resolve_entry(
            dir.path(),
            &entry("gc", SOURCE_PATH, &generation_of(&OTHER_BATCH), "b1", false),
            Some(&mut failing),
        );
        assert_eq!(
            answer
                .map(|batch| batch.as_deref().map(wire_owned))
                .map_err(|e| e.0),
            Err("the batch died".to_string())
        );
    }

    /// Each prefix resolves through its own half's function over the extraction tree: the source
    /// file's path under `gc` and under `gd`, the design document, the workflow definition from
    /// `.rigger/workflow.yml`, and the out-of-line test module only under the flag its entry
    /// records. A prefix no half owns resolves nothing.
    #[test]
    fn each_prefix_resolves_through_its_own_half_under_the_entrys_flag() {
        let dir = planted_extraction_tree(write_file);
        let from_tree = |prefix: &str, path: &str, generation: &str, excluded: bool| {
            resolved(
                dir.path(),
                &entry(prefix, path, generation, "", excluded),
                &[],
            )
            .0
        };
        for (prefix, path, excluded) in [
            ("gc", SOURCE_PATH, false),
            ("gd", SOURCE_PATH, false),
            ("gd", DOCUMENT_PATH, false),
            ("gw", WORKFLOW_PATH, false),
            ("gc", TEST_MODULE_PATH, true),
        ] {
            assert_eq!(
                from_tree(prefix, path, walked_generation(prefix, path), excluded),
                Some(pairs(walked_batch(prefix, path))),
                "{prefix}/{path}"
            );
        }
        assert_eq!(
            from_tree(
                "gc",
                TEST_MODULE_PATH,
                walked_generation("gc", TEST_MODULE_PATH),
                false
            ),
            None,
            "the test module's boundary batch is not what its bytes extract to without the flag"
        );
        assert_eq!(
            from_tree(
                "gc",
                SOURCE_PATH,
                walked_generation("gc", SOURCE_PATH),
                true
            ),
            None,
            "the source file's batch is not what its bytes extract to under the flag"
        );
        assert_eq!(
            from_tree(
                "gx",
                SOURCE_PATH,
                walked_generation("gc", SOURCE_PATH),
                false
            ),
            None
        );
    }

    /// An empty batch resolves no entry: an entry naming the generation of no events, for a
    /// design document the tree does not hold, resolves nothing rather than the empty batch.
    #[test]
    fn an_entry_naming_the_generation_of_the_empty_batch_resolves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let empty = batch_generation(&[]);
        for prefix in ["gd", "gw"] {
            assert_eq!(
                resolved(
                    dir.path(),
                    &entry(prefix, "docs/absent.md", &empty, "", false),
                    &[]
                ),
                (None, Vec::<String>::new()),
                "{prefix}"
            );
        }
    }

    /// The tree's file is read by the tree's one read rule: a `gc` path under a hidden directory
    /// is handed no bytes, so an entry of the generation its file extracts to resolves nothing
    /// and an entry of the generation of no bytes resolves.
    #[test]
    fn a_path_outside_the_walks_scope_is_read_as_no_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = ".hidden/lib.rs";
        write_file(&dir.path().join(path), OTHER_BODY.as_bytes());
        let extracted = [
            (
                "CodeEntityExtracted",
                r#"{"file":".hidden/lib.rs","name":"tree_only","kind":"function","line":1,"lang":"rust","fresh":true}"#,
            ),
            (
                "EdgeInferred",
                r#"{"file":".hidden/lib.rs","name":"","lang":"rust","fresh":true,"is_test":true}"#,
            ),
        ];
        let no_bytes = [(
            "EdgeInferred",
            r#"{"file":".hidden/lib.rs","name":"","lang":"unknown","fresh":true}"#,
        )];
        let from_tree = |generation: &str| {
            resolved(dir.path(), &entry("gc", path, generation, "", false), &[]).0
        };
        assert_eq!(from_tree(&generation_of(&extracted)), None);
        assert_eq!(from_tree(&generation_of(&no_bytes)), Some(pairs(&no_bytes)));
        // The same bytes, handed as the entry's blob, do extract to the first generation.
        assert_eq!(
            resolved(
                dir.path(),
                &entry("gc", path, &generation_of(&extracted), "b1", false),
                &[("b1", OTHER_BODY)]
            )
            .0,
            Some(pairs(&extracted))
        );
    }

    /// An entry whose payload does not parse resolves nothing and asks no source.
    #[test]
    fn an_entry_whose_payload_does_not_parse_resolves_nothing() {
        let dir = tree_holding_the_other_body();
        assert_eq!(
            resolved(
                dir.path(),
                &Event::new(TYPE_GENERATION_INGESTED, b"{}".to_vec()),
                &[("", OTHER_BODY)]
            ),
            (None, Vec::<String>::new())
        );
    }
}

#[cfg(all(test, not(feature = "symbols")))]
mod resolve_entry_stub_tests {
    use super::resolve_entry;
    use crate::contextgraph::Error;
    use crate::eventstore::Event;

    /// The light lane compiles no extraction: an entry whose path holds a file, naming a blob,
    /// resolves nothing, and the blob source is never asked.
    #[test]
    fn without_an_extraction_no_entry_resolves_and_no_blob_is_asked_for() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("lib.rs"), "fn product() {}\n").unwrap();
        let entry = Event::new(
            "GenerationIngested",
            br#"{"prefix":"gc","file":"lib.rs","generation":"h1","blob":"b1","excluded":false}"#
                .to_vec(),
        );
        let mut asked = 0;
        let mut source = |_: &str| -> Result<Option<Vec<u8>>, Error> {
            asked += 1;
            Ok(Some(b"fn product() {}\n".to_vec()))
        };
        let answer = resolve_entry(dir.path(), &entry, Some(&mut source));
        assert_eq!(
            answer.map(|batch| batch.is_none()).map_err(|e| e.0),
            Ok(true)
        );
        assert_eq!(asked, 0);
    }
}

#[cfg(all(test, feature = "symbols"))]
mod scoped_reindex_tests {
    //! Tests for [`ingest_files_batched`]: the scoped walk over the named files alone.

    use super::{ingest_files_batched, ingest_project_batched_paced};

    /// [`ingest_files_batched`] is bounded to exactly the NAMED files - an untouched sibling never
    /// reaches the sink, even though it is present and indexable. Mirrors
    /// `grounder::symbols::events::tests::file_batches_is_scoped_to_the_named_files_only` one layer
    /// up, at the walk that hands the conductor's sink its batches.
    #[test]
    fn ingest_files_batched_is_bounded_to_the_named_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn foo() {}\n").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn bar() {}\n").unwrap();
        let root = dir.path().to_str().unwrap();

        let mut seen_files: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let stats = ingest_files_batched(root, &["a.rs".to_string()], |keyed, _| {
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
        ingest_project_batched_paced(root, 1, &[], |keyed, _| {
            walked.extend(keyed.iter().map(|(k, _)| k.clone()))
        });
        let mut named: Vec<String> = Vec::new();
        ingest_files_batched(
            root,
            &["docs/architecture.md".into(), ".rigger/persona.md".into()],
            |keyed, _| named.extend(keyed.iter().map(|(k, _)| k.clone())),
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
}
