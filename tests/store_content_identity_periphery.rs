//! Periphery (contract / API / integration) tests for spec 60 criterion 4: the
//! `ContentIdentity` policy value and the HONESTY of the shared append-and-fold
//! authority under an append that writes fewer events than it was handed.
//!
//! These run OUTSIDE the crate, over the library's PUBLIC surface (`rigger::...`), so
//! they guard boundaries the inside-out unit tests are structurally blind to:
//!
//!  - nothing in-crate drives the shared authority `rigger::ingest::append_and_fold_batch`
//!    across a PARTIALLY written append, so nothing there pins the property the criterion
//!    exists for: the fold stamps only the events the store wrote, at the positions the
//!    store issued. That seam spans three modules (`eventstore` -> `ingest` ->
//!    `contextgraph`) and no single module's tests can see it;
//!  - every in-crate test of the fold uses the embedded store, whose positions are
//!    consecutive rowids. A backend whose global position is a byte offset satisfies
//!    this port (it promises DISTINCT, strictly increasing positions and never the word
//!    consecutive) and leaves gaps, and the arithmetic this criterion deletes
//!    (`base = last + 1 - n`) is wrong there even when nothing is suppressed. Only a
//!    consumer-implemented port can exhibit that, so it is pinned here;
//!  - `Appended` and `ContentIdentity` are new PUBLIC types. Their edges - a report
//!    whose written events are not a prefix of the batch, a trailing suppression, an
//!    all-suppressed report, a type the policy does not cover - are reachable by any
//!    external consumer and are asserted through the public API rather than through the
//!    one policy the project happens to configure;
//!  - the seams that now have to express "the store wrote nothing" - `emit_event`,
//!    `progress::record`, `spawn_store::record_result` - are module boundaries, and the
//!    failure they guard against (folding at a fabricated position `0`, which the
//!    graph's applied ledger records as permanently applied) is only observable from
//!    outside, by watching what the projection is handed.
//!
//! Everything driven here - the store, the shared fold authority, the projection trait,
//! the spawn and progress seams - is compiled UNCONDITIONALLY, so this whole suite runs
//! in BOTH feature lanes.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use rigger::contextgraph::{Error as CgError, Projection, TYPE_DECISION_MADE};
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{
    Appended, ContentIdentity, Direction, Error as StoreError, Event, EventStore, ExpectedRevision,
    Filter, Position, Revision, Subscription,
};
use rigger::ingest::append_and_fold_batch;

// ---------------------------------------------------------------------------
// Doubles and fixtures
// ---------------------------------------------------------------------------

/// A `Projection` that records the positions handed to each `apply_batch` call (grouped
/// per call) and the positions handed to each per-event `apply`, so a test can prove
/// exactly WHICH events were folded and at WHICH positions - the two facts a partially
/// suppressed append can get wrong.
#[derive(Default)]
struct CapturingProjection {
    batches: Mutex<Vec<Vec<Position>>>,
    singles: Mutex<Vec<Position>>,
}

impl CapturingProjection {
    /// Every position folded, in fold order, however it was folded.
    fn folded(&self) -> Vec<Position> {
        let mut out: Vec<Position> = self
            .batches
            .lock()
            .unwrap()
            .iter()
            .flatten()
            .copied()
            .collect();
        out.extend(self.singles.lock().unwrap().iter().copied());
        out
    }

    /// How many times `apply_batch` was called - a fold that folds NOTHING must not
    /// call it at all, so this separates "folded an empty batch" from "did not fold".
    fn batch_calls(&self) -> usize {
        self.batches.lock().unwrap().len()
    }
}

impl Projection for CapturingProjection {
    fn apply(&self, e: &Event, _access: rigger::contextgraph::FoldAccess) -> Result<(), CgError> {
        self.singles.lock().unwrap().push(e.position);
        Ok(())
    }
    fn apply_batch(
        &self,
        events: &[Event],
        _access: rigger::contextgraph::FoldAccess,
    ) -> Result<(), CgError> {
        self.batches
            .lock()
            .unwrap()
            .push(events.iter().map(|e| e.position).collect());
        Ok(())
    }
    crate::projection_reads_nothing!();
}

/// A consumer-implemented `EventStore` that reports EXACTLY the placements it was built
/// with. It is the only way to exhibit two port-legal behaviors the embedded store
/// cannot: positions with GAPS (a backend whose global position is a byte offset), and
/// an append that writes nothing at all on a seam where no guard is configured.
///
/// It answers appends only. Its reads return an error rather than an empty success,
/// because a silent empty read would let a test pass by folding nothing for the wrong
/// reason; nothing on the paths under test reads through it. The one seam that MUST read
/// before it appends (`record_result_if_absent` reads the stream to decide whether a
/// result already exists) is served by [`PortDouble::over_an_empty_stream`], which
/// answers that one read with an empty stream and nothing else.
struct PortDouble {
    report: Vec<Option<Position>>,
    handed: AtomicUsize,
    /// Whether the double insists its report answers the batch it is handed. False only
    /// for the deliberate liar below.
    exact: bool,
    /// Whether `read_stream` answers instead of refusing. True only for the seams that
    /// READ before they append; what it answers is [`PortDouble::replayed`].
    reads_empty: bool,
    /// What `read_stream` replays when it answers at all. Empty for a seam whose decision
    /// is "nothing recorded yet"; a prior state for a seam that must FIND something and
    /// then write about it.
    replayed: Vec<Event>,
}

impl PortDouble {
    fn new(report: Vec<Option<Position>>) -> Self {
        PortDouble::built(report, true, None)
    }

    /// A port answering `report`, `exact` about its slot count, whose reads replay `replayed`
    /// (then whose append writes nothing) when given, and are live otherwise.
    fn built(report: Vec<Option<Position>>, exact: bool, replayed: Option<Vec<Event>>) -> Self {
        PortDouble {
            report,
            handed: AtomicUsize::new(0),
            exact,
            reads_empty: replayed.is_some(),
            replayed: replayed.unwrap_or_default(),
        }
    }

    /// A port that reports a DIFFERENT number of slots than it was handed. It is
    /// port-ILLEGAL - the contract is one slot per handed event - and it exists so the
    /// fold authority can be driven against a report it must refuse instead of absorb.
    fn miscounting(report: Vec<Option<Position>>) -> Self {
        PortDouble::built(report, false, None)
    }

    /// A port whose stream is EMPTY and whose append writes nothing - the exact state a
    /// compare-and-append seam must not mistake for "someone else already recorded it".
    fn over_an_empty_stream(report: Vec<Option<Position>>) -> Self {
        PortDouble::over_a_stream(Vec::new(), report)
    }

    /// A port that REPLAYS `events` to a read and then writes nothing on the append that
    /// follows - for a seam whose write is a decision about state it had to read first.
    fn over_a_stream(events: Vec<Event>, report: Vec<Option<Position>>) -> Self {
        PortDouble::built(report, true, Some(events))
    }
}

fn unreadable() -> StoreError {
    StoreError::Backend("the port double answers appends only".into())
}

impl EventStore for PortDouble {
    fn append(
        &self,
        _stream: &str,
        _expected: ExpectedRevision,
        events: &[Event],
    ) -> Result<Appended, StoreError> {
        self.handed.fetch_add(events.len(), Ordering::SeqCst);
        if self.exact {
            assert_eq!(
                events.len(),
                self.report.len(),
                "the double is built for one exact batch size"
            );
        }
        Ok(Appended::from_placements(self.report.clone()))
    }
    fn read_stream(
        &self,
        _stream: &str,
        _from: Revision,
        _dir: Direction,
    ) -> Result<Vec<Event>, StoreError> {
        if self.reads_empty {
            return Ok(self.replayed.clone());
        }
        Err(unreadable())
    }
    fn read_all(
        &self,
        _from: Position,
        _dir: Direction,
        _filter: &Filter,
    ) -> Result<Vec<Event>, StoreError> {
        Err(unreadable())
    }
    fn subscribe_all(&self, _from: Position, _filter: &Filter) -> Result<Subscription, StoreError> {
        Err(unreadable())
    }
    fn subscribe_stream(&self, _stream: &str, _from: Revision) -> Result<Subscription, StoreError> {
        Err(unreadable())
    }
    fn last_position(
        &self,
        _stream: &str,
        event_type: &str,
    ) -> Result<Option<Revision>, StoreError> {
        if self.reads_empty {
            return Ok(self
                .replayed
                .iter()
                .rev()
                .find(|e| e.type_ == event_type)
                .map(|e| e.revision));
        }
        Err(unreadable())
    }
    fn read_stream_typed(
        &self,
        _stream: &str,
        from: Revision,
        selection: rigger::eventstore::TypeSelection,
    ) -> Result<Vec<Event>, StoreError> {
        if self.reads_empty {
            return Ok(self
                .replayed
                .iter()
                .filter(|e| {
                    let named = |types: &[&str]| types.contains(&e.type_.as_str());
                    e.revision >= from
                        && match selection {
                            rigger::eventstore::TypeSelection::Only(types) => named(types),
                            rigger::eventstore::TypeSelection::Except(types) => !named(types),
                        }
                })
                .cloned()
                .collect());
        }
        Err(unreadable())
    }
}

// ---------------------------------------------------------------------------
// The fold seam: ingest -> eventstore -> contextgraph
// ---------------------------------------------------------------------------

/// THE FALSIFICATION OF THE DELETED ARITHMETIC, and the reason a consumer-implemented
/// port earns its place here: this backend's positions have GAPS, which the port allows
/// (it promises distinct, strictly increasing positions - a backend whose `$all`
/// position is a byte offset satisfies that) and which every in-crate test, driving the
/// embedded store's consecutive rowids, cannot exhibit.
///
/// `base = last + 1 - n` would fold this batch at `[898, 899, 900]`. Two of those three
/// positions belong to no event of this batch, and one of them may well belong to a
/// DIFFERENT event the log already holds. The assertion below is red under that
/// arithmetic and green only when every stamp comes from the store's own report.
#[test]
fn the_fold_uses_the_reported_positions_on_a_backend_whose_positions_have_gaps() {
    let gapped = PortDouble::new(vec![Some(100), Some(250), Some(900)]);
    let cap = CapturingProjection::default();
    let events: Vec<Event> = (0..3)
        .map(|i| Event::new("Gapped", vec![i as u8]))
        .collect();

    let appended = append_and_fold_batch(&gapped, Some(&cap as &dyn Projection), "run", &events)
        .expect("the append succeeds");
    assert_eq!(appended.fold, rigger::contextgraph::Fold::Folded);
    let appended = appended.appended;

    assert_eq!(
        cap.folded(),
        vec![100, 250, 900],
        "every folded event carries the position the STORE reported, gaps and all"
    );
    assert_eq!(
        appended.last(),
        Some(900),
        "the reported last position is the greatest one written"
    );
    assert_eq!(
        gapped.handed.load(Ordering::SeqCst),
        3,
        "the whole batch reached the store in ONE append"
    );
}

/// A port that reports writing nothing - which any adapter may do, and which the seams
/// above must express rather than paper over - folds nothing. A fold at a fabricated
/// `0` would mark position 0 applied forever in a ledger keyed by position.
#[test]
fn a_port_that_wrote_nothing_is_never_folded_at_a_fabricated_position() {
    let silent = PortDouble::new(vec![None, None]);
    let cap = CapturingProjection::default();
    let events: Vec<Event> = (0..2)
        .map(|i| Event::new("Silent", vec![i as u8]))
        .collect();

    let appended = append_and_fold_batch(&silent, Some(&cap as &dyn Projection), "run", &events)
        .expect("an append that wrote nothing is not an error");
    assert_eq!(
        appended.fold,
        rigger::contextgraph::Fold::Folded,
        "nothing written is nothing to fold"
    );
    let appended = appended.appended;

    assert_eq!(appended.written(), 0);
    assert_eq!(appended.last(), None);
    assert!(
        cap.folded().is_empty(),
        "nothing was written, so nothing is folded - in particular nothing at position 0"
    );
    assert_eq!(cap.batch_calls(), 0, "no fold call is made at all");
}

/// A report that does not ANSWER the batch is refused, not absorbed.
///
/// The fold authority stamps positions by ZIPPING the report against the batch it handed
/// in, so one slot per handed event is not a nicety - it is what makes slot `i` mean
/// event `i`. A report of a different length silently re-aligns every slot after the
/// discrepancy onto the wrong event, and the graph's ledger is keyed by position, so the
/// misattribution is permanent. Iterating the report cannot notice this on its own: a
/// slot index the batch cannot answer simply yields nothing, which reads exactly like a
/// suppression. So the check is explicit, it happens BEFORE anything is folded, and it
/// names both counts.
#[test]
fn a_report_that_does_not_answer_the_batch_is_refused_rather_than_folded() {
    let miscounting = PortDouble::miscounting(vec![Some(7)]);
    let cap = CapturingProjection::default();
    let events: Vec<Event> = (0..3)
        .map(|i| Event::new("Derived", vec![i as u8]))
        .collect();

    let err = append_and_fold_batch(&miscounting, Some(&cap as &dyn Projection), "run", &events)
        .expect_err("a report that cannot name what was written is not a smaller fold");

    let message = err.to_string();
    assert!(
        message.contains('1') && message.contains('3'),
        "the refusal must name both counts so the broken adapter is identifiable: {message}"
    );
    assert!(
        cap.folded().is_empty() && cap.batch_calls() == 0,
        "and nothing is folded from a report that cannot be trusted to name a position"
    );
}

// ---------------------------------------------------------------------------
// The new public types, at their edges
// ---------------------------------------------------------------------------

/// `Appended` is the report every caller zips against the batch it handed in, so its
/// edges are API surface: the written events are NOT necessarily a prefix of the batch,
/// a suppression may be the LAST slot, and the indices it yields must be indices into
/// the CALLER'S batch rather than a running count of the written ones. Getting that
/// wrong stamps the right positions onto the wrong events - a fold that is silently,
/// permanently misattributed.
#[test]
fn the_append_report_is_a_consistent_whole_at_its_edges() {
    let empty = Appended::default();
    assert_eq!(empty.handed(), 0);
    assert_eq!(empty.written(), 0);
    assert_eq!(empty.last(), None);
    assert_eq!(empty.placed().count(), 0);
    assert!(empty.placements().is_empty());
    assert_eq!(
        Appended::all(Vec::new()),
        empty,
        "an append of no events and a default report are the same answer"
    );

    let all = Appended::all(vec![7, 9, 11]);
    assert_eq!(all.handed(), 3);
    assert_eq!(all.written(), 3);
    assert_eq!(all.last(), Some(11));
    assert_eq!(
        all.placed().collect::<Vec<_>>(),
        vec![(0, 7), (1, 9), (2, 11)]
    );

    // Suppression in the MIDDLE: the indices are the caller's, not a count of writes.
    let holed = Appended::from_placements(vec![None, Some(7), None, Some(9)]);
    assert_eq!(holed.handed(), 4);
    assert_eq!(holed.written(), 2);
    assert_eq!(
        holed.placed().collect::<Vec<_>>(),
        vec![(1, 7), (3, 9)],
        "each written event is named by its index in the batch the caller handed in"
    );
    assert_eq!(holed.last(), Some(9));

    // Suppression in the LAST slot: `last` is the last event WRITTEN, not the last slot.
    let trailing = Appended::from_placements(vec![Some(5), Some(9), None]);
    assert_eq!(trailing.last(), Some(9));
    assert_eq!(trailing.written(), 2);
    assert_eq!(trailing.handed(), 3);

    // Nothing written at all: an absence, whatever the batch size.
    let none = Appended::from_placements(vec![None; 4]);
    assert_eq!(none.last(), None);
    assert_eq!(none.written(), 0);
    assert_eq!(none.handed(), 4);

    assert_eq!(
        holed.clone(),
        holed,
        "the report is a value: clonable and equal"
    );
    assert_ne!(holed, trailing);
    assert!(
        format!("{holed:?}").contains("Appended"),
        "the report is debuggable, so a failing assertion says what it saw"
    );
}

/// `ContentIdentity` is CONFIGURATION handed to a store, so its accessors are the whole
/// contract between the layer that owns the derived index and the store that maintains it.
/// The TYPE test must be an EXACT match and never a prefix or a case-folded one.
#[test]
fn the_content_identity_policy_answers_exactly_what_it_was_configured_with() {
    let identity = ContentIdentity::new("replay_key", ["Alpha", "Beta"]);

    assert_eq!(identity.meta_key(), "replay_key");
    assert_eq!(identity.types(), ["Alpha".to_string(), "Beta".to_string()]);

    assert!(identity.covers("Alpha"));
    assert!(identity.covers("Beta"));
    for foreign in ["alpha", "ALPHA", "Alph", "AlphaExtra", "", "Gamma"] {
        assert!(
            !identity.covers(foreign),
            "{foreign:?} is not a configured type"
        );
    }

    // The policy is a value the composition root may clone into several stores.
    let cloned = identity.clone();
    assert!(cloned.covers("Alpha"));
}

// ---------------------------------------------------------------------------
// The seams that now have to express "the store wrote nothing"
// ---------------------------------------------------------------------------

/// A RESULT SEAM THAT CANNOT SAY WHERE IT WROTE HAS NOT RECORDED ANYTHING, and must say
/// so. `record_result` answers a bare `Position` because a run-lifecycle event is outside
/// every content-identity policy by construction (the type test is asked first), so a
/// store reporting that it wrote nothing here is a BROKEN PORT rather than a case to
/// absorb. The alternative - handing back a fabricated `0` - is the worst of the three
/// outcomes: a lost self-report that reads as a recorded one and whose cited position
/// belongs to a different event entirely.
///
/// A consumer-implemented port is the only way to reach this arm, since no policy the
/// composition root could configure will ever suppress a spawn result.
#[test]
fn the_result_seam_reports_a_store_that_wrote_nothing_as_an_error_naming_what_happened() {
    let silent = PortDouble::new(vec![None]);
    let result = rigger::spawn::SpawnResult::ok("u1/impl#0", "done");

    let err = rigger::spawn_store::record_result(&silent, &result)
        .expect_err("a store that wrote nothing has not recorded the result");
    let message = err.to_string();
    assert!(
        message.contains("nothing"),
        "the error says the store wrote nothing rather than reporting a position: {message}"
    );
    assert!(
        message.contains("SpawnResult") && message.contains("u1/impl#0"),
        "and names the event whose write was lost AND whose it was, so the seam is \
         identifiable: {message}"
    );
}

/// THE OTHER TWO SEAMS THAT HAND BACK A BARE POSITION, held to the same obligation.
///
/// `record_result` is not the only run-lifecycle write that must be able to say "the
/// store wrote nothing": `park_in_run` records the SPAWN REQUEST every later step reads
/// the frontier from, and `record_result_if_absent` is the death courier's atomic
/// compare-and-append. Both were rewritten by this criterion to stop deriving a position
/// and to ask the store instead, so both acquired the same new arm, and neither is
/// reachable from any policy the composition root could configure - only a
/// consumer-implemented port gets there.
///
/// The `if_absent` half carries the sharper hazard, and it is a hazard of MEANING rather
/// than of arithmetic. That seam already answers `Ok(None)`, and `Ok(None)` means "a
/// result was already recorded, so I deliberately wrote nothing" - the idempotent no-op
/// the courier wants. A store that wrote nothing collapsed into that same answer would
/// report a LOST write as a successful no-op, and the courier would move on believing a
/// worker's death was recorded. The two absences must therefore stay distinguishable:
/// one is a decision, the other is a failure.
#[test]
fn the_park_and_compare_and_append_seams_refuse_a_store_that_wrote_nothing() {
    let request = common::spawn_request("u1", "build", "impl", 0, "do the thing");
    let silent = PortDouble::new(vec![None]);

    let err = rigger::spawn_store::park_in_run(&silent, &request, "run-1")
        .expect_err("a parked spawn nobody can locate has not been parked");
    let message = err.to_string();
    assert!(
        message.contains("nothing"),
        "the error says the store wrote nothing rather than citing a position: {message}"
    );
    assert!(
        message.contains(&request.id),
        "and names the spawn whose park was lost, so the operator knows what is missing: \
         {message}"
    );

    // The compare-and-append seam reads first (an empty stream: no result recorded yet),
    // then appends - and the append writes nothing.
    let quiet = PortDouble::over_an_empty_stream(vec![None]);
    let result = rigger::spawn::SpawnResult::ok("u1/impl#0", "done");
    let outcome = rigger::spawn_store::record_result_if_absent(&quiet, &result);
    assert!(
        outcome.is_err(),
        "a store that wrote nothing must never be reported as the idempotent no-op - \
         `Ok(None)` there means a result already exists, so absorbing a lost write into it \
         tells the death courier a report landed when none did; got {outcome:?}"
    );
    let message = outcome.unwrap_err().to_string();
    assert!(
        message.contains("nothing") && message.contains(&result.id),
        "and the failure names what was lost and whose it was: {message}"
    );
}

/// THE RUN BOUNDARY, driven through the PUBLIC entries a consumer actually calls.
///
/// The two writes this module makes are not ordinary records. A run id is not a local
/// value: every later `current_run_id`, `current_run_base` and spawn attribution
/// partitions the WHOLE log against the boundary event these writes record. A store that
/// wrote nothing, and a caller that handed the id back anyway, gives the rest of the run a
/// boundary the log does not contain - and nothing downstream re-checks it, so the run
/// finishes reading a partition that was never there.
///
/// Both writes reached the store through a report they discarded, which is why they need
/// pinning from OUT HERE: a discarded report keeps compiling when the port's answer
/// changes, so no signature and no in-module read can tell you whether the seam still
/// looks. The property is a property of the API: no public run entry may hand back an id,
/// or report a re-pin, for a boundary that was never written.
///
/// The re-pin half needs a store that FINDS a live run and then loses the write about it,
/// so the fixture is minted by the module itself on a real store and replayed - a
/// hand-built RunStarted would prove only that this test can serialize one.
#[test]
fn no_public_run_entry_reports_a_boundary_the_store_never_wrote() {
    let criteria = ["build the thing".to_string()];

    let silent = PortDouble::new(vec![None]);
    let message = rigger::run_store::start_fresh(&silent, &criteria, "hash-A", "base-sha", "", "")
        .expect_err("a run whose boundary was never written has not started")
        .to_string();
    assert!(
        message.contains("nothing"),
        "the mint reports the lost write rather than handing back a run id for a boundary \
         the log does not hold: {message}"
    );

    // The same answer through the entry the CLI actually calls, which mints over an empty
    // store: a caller that only ever uses the pinned entry must not get a run id either.
    let silent = PortDouble::over_an_empty_stream(vec![None]);
    let message = rigger::run_store::ensure_started_pinned(
        &silent, &criteria, "hash-A", false, "base-sha", "", "",
    )
    .expect_err("the pinned entry mints on an empty store and inherits the same answer")
    .to_string();
    assert!(
        message.contains("nothing"),
        "so the mint cannot be laundered through the pinned entry: {message}"
    );

    // THE RE-PIN. A live run whose definition drifted, rebased: the supersession is
    // recorded and the run continues under the NEW definition. If that record is lost and
    // the caller reports `Rebased` anyway, the run replays a definition the log still pins
    // to the old hash - the silent mid-campaign reconfiguration this pinning exists to
    // stop, now invisible in the very log that was supposed to show it.
    let live = Store::open(":memory:").expect("an in-memory store opens");
    rigger::run_store::start_fresh(&live, &criteria, "hash-A", "base-sha", "", "")
        .expect("a real run mints");
    let recorded = live
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .expect("the boundary reads back");
    assert_eq!(recorded.len(), 1, "the fixture is one real RunStarted");

    let drifted = PortDouble::over_a_stream(recorded, vec![None]);
    let message = rigger::run_store::ensure_started_pinned(
        &drifted, &criteria, "hash-B", true, "base-sha", "", "",
    )
    .expect_err("a supersession nobody can locate has not superseded anything")
    .to_string();
    assert!(
        message.contains("nothing"),
        "the re-pin reports the lost write rather than announcing a rebase the log does not \
         record: {message}"
    );
}

/// A driver that must never be reached. The canary opens its batch with a MARKER before it
/// scores anything, so a run whose very first write is lost has to stop there; a spawn
/// after that would mean the seam drove on.
struct NeverSpawns;

impl rigger::conductor::AgentDriver for NeverSpawns {
    fn spawn(
        &self,
        _agent: &rigger::config::AgentDef,
        _prompt: &str,
        _opts: &rigger::conductor::SpawnOpts,
        _emit: &dyn Fn(&str, serde_json::Value) -> Result<(), rigger::conductor::Error>,
    ) -> Result<rigger::conductor::AgentResult, rigger::conductor::Error> {
        panic!("the canary must not score anything once its batch marker was lost")
    }
}

/// THE CANARY'S RECORD, through the public entry, on a port that wrote nothing.
///
/// The canary is a MEASUREMENT, and the events it appends are the only durable trace it
/// leaves: the returned report is for the command's summary print and is gone at process
/// exit, so `rigger stats --canary` reads the log or reads nothing. A write this seam lost
/// is a measurement that reads as taken to whoever watched it run and cannot be found
/// afterwards - the worst shape a quality signal can take, because it is trusted.
///
/// The seam is reachable from out here because both the store and the driver are injected,
/// which is the point: this is the composition a consumer wires, and the batch marker is
/// written BEFORE the first spawn, so a driver that refuses to run proves the loss stops
/// the run rather than being carried past it.
#[test]
fn the_canary_records_nothing_it_cannot_find_afterwards() {
    let silent = PortDouble::new(vec![None]);
    let panel = rigger::config::ReviewPanel {
        adjudicator: "adj".into(),
        ..Default::default()
    };
    let outcome = rigger::canary_store::run_canary(
        &silent,
        &NeverSpawns,
        &rigger::config::Config::default(),
        &panel,
        &[],
        rigger::canary_store::default_jobs(),
        &|_, _| {},
    );
    let message = match outcome {
        Ok(report) => panic!(
            "the canary returned batch {} as a measurement, but the log holds no marker for \
             it - a scorecard that reads as taken and cannot be found afterwards",
            report.batch
        ),
        Err(e) => e.to_string(),
    };
    assert!(
        message.contains("nothing"),
        "the failure says the store wrote nothing rather than returning a report whose \
         batch the log does not hold: {message}"
    );
}

// ---------------------------------------------------------------------------
// Where the guard meets the one-meaning-of-an-absence rule
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// The operator's surface: the two commands that now print what the store wrote
// ---------------------------------------------------------------------------

/// A throwaway project the compiled binary will accept: its own git repo (so the store's
/// project identity resolves exactly as a real project's does), a pinned `project.id` so
/// the stream this test reads back is the stream the binary wrote to whatever the temp
/// directory is called, and an INITIALIZED event log - the binary refuses to fabricate one.
fn cli_project() -> tempfile::TempDir {
    let dir = temp_project();
    let rigger_dir = dir.path().join(".rigger");
    std::fs::create_dir_all(&rigger_dir).expect("create .rigger");
    std::fs::write(rigger_dir.join("project.id"), "u4-cli-surface").expect("pin the identity");
    // Opening the store creates the schema the binary then appends to.
    init_event_log(dir.path());
    dir
}

// The compiled `rigger` binary under test is located at RUNTIME by the shared authority in
// `tests/common`: a path baked in at compile time goes stale the moment the target dir moves,
// and every suite that spawns the product then dies with a bare NotFound.
mod common;
use common::cli::{init_event_log, temp_project};

/// Run `rigger <args...>` in `root` through the COMPILED binary, returning its stdout.
fn run_rigger(root: &std::path::Path, args: &[&str]) -> String {
    let state = tempfile::tempdir().expect("a temp XDG_STATE_HOME");
    let out = common::rigger_courier()
        .args(args)
        .current_dir(root)
        // Never let a short-lived invocation spawn a real dashboard, and never let it
        // register a phantom instance in the operator's machine-global registry.
        .env("RIGGER_NO_DASH", "1")
        .env("XDG_STATE_HOME", state.path())
        .output()
        .expect("the rigger binary runs");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "rigger {args:?} failed: {stderr}\n{stdout}"
    );
    stdout
}

/// The position an operator-facing line cites, parsed out of `(position N)`.
fn cited_position(line: &str) -> Position {
    line.split_once("(position ")
        .and_then(|(_, rest)| rest.split(')').next())
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or_else(|| panic!("the line must cite a position an operator can use: {line}"))
}

/// Every position the namespaced `stream` under `root` actually holds, read back the way
/// the binary reads it.
fn cli_held(root: &std::path::Path, db: &str, stream: &str) -> Vec<Event> {
    let backend = Store::open(
        root.join(".rigger")
            .join(db)
            .to_str()
            .expect("a utf-8 store path"),
    )
    .expect("the store opens");
    let store = Namespaced::new(&backend, "u4-cli-surface");
    store
        .read_stream(stream, 0, Direction::Forward)
        .expect("the stream reads back")
}

/// THE OPERATOR'S SURFACE, driven through the COMPILED BINARY - the only place the two
/// commands this criterion rewrote can be seen at all.
///
/// `rigger emit` and `rigger progress` print ONE line each: the position the store issued
/// for the event they wrote. Every library-level test in this file drives the seam
/// directly, so none of them can see what the command prints, and printing a position is
/// not decoration: it is the handle an operator (and the dashboard, and a later citation)
/// uses to find the event in the log. A line citing a position the log does not hold is
/// worse than no line at all, and a success line for an event that was never written is
/// worse still - so an absence reaches the operator as a failure, never as a line.
///
/// So the citation is checked against what the store HOLDS, not against a format: emit
/// twice and progress once, and every cited position must name the very event the command
/// wrote. The second emit is the falsifying half - two byte-identical decisions are two
/// facts at two positions, because the shipped composition root configures no
/// content-identity policy over the run stream and a domain type is outside every policy
/// it could configure. A command that printed the first position again would pass a
/// format check and fail this one.
#[test]
fn the_built_binary_cites_only_positions_the_log_actually_holds() {
    let project = cli_project();
    let root = project.path();
    let decision = r#"{"id":"d1","summary":"a decision"}"#;

    let first_out = run_rigger(root, &["emit", TYPE_DECISION_MADE, decision]);
    let first = cited_position(&first_out);
    assert!(
        first_out.contains("folded it into the context graph"),
        "the emit that wrote reports that it wrote, and where: {first_out}"
    );

    let second_out = run_rigger(root, &["emit", TYPE_DECISION_MADE, decision]);
    let second = cited_position(&second_out);
    assert!(
        first < second,
        "two identical decisions are two facts at two positions, never one cited twice: \
         {first} then {second}"
    );

    let run = cli_held(root, "events.db", rigger::conductor::STREAM);
    assert_eq!(
        run.iter().map(|e| e.position).collect::<Vec<_>>(),
        vec![first, second],
        "the log holds exactly the events the two commands claimed to write, at exactly \
         the positions they cited"
    );
    assert!(
        run.iter().all(|e| e.type_ == TYPE_DECISION_MADE),
        "and each cited position names the decision that was emitted, not some neighbour"
    );

    let progress_out = run_rigger(root, &["progress", "u1/impl#0", "did a thing"]);
    let recorded = cited_position(&progress_out);
    assert!(
        progress_out.contains("progress recorded for u1/impl#0"),
        "the progress line names the spawn it recorded for: {progress_out}"
    );
    assert_eq!(
        cli_held(root, "progress.db", rigger::progress::STREAM)
            .iter()
            .map(|e| e.position)
            .collect::<Vec<_>>(),
        vec![recorded],
        "and the SEPARATE progress log holds that one report at the position the command \
         cited"
    );
    assert_eq!(
        cli_held(root, "events.db", rigger::conductor::STREAM).len(),
        2,
        "a progress report is recorded in its own store and never in the run stream, so the \
         position it cites belongs to a different log and the two can never be confused"
    );
}

// ---------------------------------------------------------------------------
// The policy is a PORT, and the port is only as good as what it refuses
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// The degradation mark: what a guard that has stopped defending says, and where
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// The policy is CONFIGURATION, so a log outlives the policy that guarded it
// ---------------------------------------------------------------------------
