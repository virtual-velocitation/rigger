//! The folding store that moves run events into the graph, with the ingest vocabulary the
//! domain defines re-exported beside it.

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
