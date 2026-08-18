//! Periphery (contract / API / integration) tests for spec 60 criterion 4: the
//! STORAGE-LEVEL CONTENT-IDENTITY GUARD and the HONESTY of the shared append-and-fold
//! authority under an append that writes fewer events than it was handed.
//!
//! These run OUTSIDE the crate, over the library's PUBLIC surface (`rigger::...`), so
//! they guard boundaries the inside-out unit tests are structurally blind to:
//!
//!  - the implementer's suppression tests drive `sqlite::Store::append` DIRECTLY and
//!    assert row counts and per-event slots. Nothing there drives the shared authority
//!    `rigger::ingest::append_and_fold_batch` across a PARTIALLY suppressed append, so
//!    nothing pins the property the criterion exists for: the fold stamps only the
//!    events the store wrote, at the positions the store issued. That seam spans three
//!    modules (`eventstore` -> `ingest` -> `contextgraph`) and no single module's tests
//!    can see it;
//!  - every in-crate test of the fold uses the embedded store, whose positions are
//!    consecutive rowids. A backend whose global position is a byte offset satisfies
//!    this port (it promises DISTINCT, strictly increasing positions and never the word
//!    consecutive) and leaves gaps, and the arithmetic this criterion deletes
//!    (`base = last + 1 - n`) is wrong there even when nothing is suppressed. Only a
//!    consumer-implemented port can exhibit that, so it is pinned here;
//!  - `Appended` and `ContentIdentity` are new PUBLIC types. Their edges - a report
//!    whose written events are not a prefix of the batch, a trailing suppression, an
//!    all-suppressed report, a policy whose split answers `None` - are reachable by any
//!    external consumer and are asserted through the public API rather than through the
//!    one policy the project happens to configure;
//!  - the guard is configured with a POLICY. The store must therefore own no key format
//!    of its own, which is only provable by driving it with a key shape this project
//!    never mints;
//!  - the seams that now have to express "the store wrote nothing" - `emit_event`,
//!    `progress::record`, `spawn::record_result` - are module boundaries, and the
//!    failure they guard against (folding at a fabricated position `0`, which the
//!    graph's applied ledger records as permanently applied) is only observable from
//!    outside, by watching what the projection is handed.
//!
//! Everything driven here - the store, the shared fold authority, the projection trait,
//! the spawn and progress seams - is compiled UNCONDITIONALLY, so this whole suite runs
//! in BOTH feature lanes.

use std::ops::Range;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use rigger::contextgraph::{
    Error as CgError, Graph, Projection, TYPE_CODE_ENTITY_EXTRACTED, TYPE_DECISION_MADE,
    TYPE_EDGE_INFERRED, TYPE_REVIEW_FINDING,
};
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{
    Appended, ContentIdentity, Direction, Error as StoreError, Event, EventStore, ExpectedRevision,
    Filter, Position, Revision, Subscription, GUARD_DEGRADED_NO_INDEX, GUARD_DEGRADED_UNDETERMINED,
    META_GUARD_DEGRADED,
};
use rigger::ingest::{append_and_fold_batch, DERIVED_INDEX_TYPES, META_REPLAY_KEY};

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
    fn apply(&self, e: &Event) -> Result<(), CgError> {
        self.singles.lock().unwrap().push(e.position);
        Ok(())
    }
    fn apply_batch(&self, events: &[Event]) -> Result<(), CgError> {
        self.batches
            .lock()
            .unwrap()
            .push(events.iter().map(|e| e.position).collect());
        Ok(())
    }
    fn subgraph(&self, _seed: &[String], _depth: i64) -> Result<Graph, CgError> {
        Ok(Graph::default())
    }
    fn resolve(&self, _mention: &str) -> Result<Option<String>, CgError> {
        Ok(None)
    }
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
        PortDouble {
            report,
            handed: AtomicUsize::new(0),
            exact: true,
            reads_empty: false,
            replayed: Vec::new(),
        }
    }

    /// A port that reports a DIFFERENT number of slots than it was handed. It is
    /// port-ILLEGAL - the contract is one slot per handed event - and it exists so the
    /// fold authority can be driven against a report it must refuse instead of absorb.
    fn miscounting(report: Vec<Option<Position>>) -> Self {
        PortDouble {
            report,
            handed: AtomicUsize::new(0),
            exact: false,
            reads_empty: false,
            replayed: Vec::new(),
        }
    }

    /// A port whose stream is EMPTY and whose append writes nothing - the exact state a
    /// compare-and-append seam must not mistake for "someone else already recorded it".
    fn over_an_empty_stream(report: Vec<Option<Position>>) -> Self {
        PortDouble::over_a_stream(Vec::new(), report)
    }

    /// A port that REPLAYS `events` to a read and then writes nothing on the append that
    /// follows - for a seam whose write is a decision about state it had to read first.
    fn over_a_stream(events: Vec<Event>, report: Vec<Option<Position>>) -> Self {
        PortDouble {
            report,
            handed: AtomicUsize::new(0),
            exact: true,
            reads_empty: true,
            replayed: events,
        }
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
}

/// Split a `<prefix>/<file>@<hash>#<i>` content key into `(the prefix every key naming
/// the same file begins with, the content generation)`. This is the shape the project's
/// ingest layer mints; it is written HERE, in the test, because the policy is injected
/// configuration and the store must never carry a key format of its own.
fn path_subject_of(key: &str) -> Option<(Range<usize>, Range<usize>)> {
    let (prefix, rest) = key.split_once('/')?;
    if prefix.is_empty() || rest.is_empty() {
        return None;
    }
    // From the RIGHT: a real path may itself contain `@` or `#`.
    let (head, index) = key.rsplit_once('#')?;
    if index.is_empty() || !index.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (file, hash) = head.rsplit_once('@')?;
    if file.len() <= prefix.len() + 1 || hash.is_empty() {
        return None;
    }
    let subject_end = file.len() + 1; // through the `@` that ends the subject
    Some((0..subject_end, subject_end..subject_end + hash.len()))
}

/// A content-key shape this project never mints: `<subject>|<generation>`. Used to prove
/// the store parses nothing itself.
fn pipe_subject_of(key: &str) -> Option<(Range<usize>, Range<usize>)> {
    let (subject, generation) = key.split_once('|')?;
    if subject.is_empty() || generation.is_empty() {
        return None;
    }
    let subject_end = subject.len() + 1;
    Some((0..subject_end, subject_end..subject_end + generation.len()))
}

/// The policy the project would configure: its real metadata key and its real derived
/// index types, so the guard is exercised against the vocabulary it will actually carry.
fn project_policy() -> ContentIdentity {
    ContentIdentity::new(META_REPLAY_KEY, DERIVED_INDEX_TYPES, path_subject_of)
}

fn guarded() -> Store {
    Store::open(":memory:")
        .expect("an in-memory store opens")
        .with_content_identity(project_policy())
}

fn keyed(type_: &str, key: &str) -> Event {
    Event::new(type_, b"payload".to_vec()).with_meta(META_REPLAY_KEY, key)
}

/// One file's batch at one content generation - the shape an ingest walk emits.
fn batch(file: &str, hash: &str) -> Vec<Event> {
    vec![
        keyed(TYPE_CODE_ENTITY_EXTRACTED, &format!("gc/{file}@{hash}#0")),
        keyed(TYPE_EDGE_INFERRED, &format!("gc/{file}@{hash}#1")),
    ]
}

/// Every position the store actually holds on `stream`, in log order.
fn held_positions(store: &dyn EventStore, stream: &str) -> Vec<Position> {
    store
        .read_stream(stream, 0, Direction::Forward)
        .expect("read must succeed")
        .iter()
        .map(|e| e.position)
        .collect()
}

// ---------------------------------------------------------------------------
// The fold seam: ingest -> eventstore -> contextgraph
// ---------------------------------------------------------------------------

/// THE SEAM THE CRITERION EXISTS FOR, driven end to end across three modules.
///
/// A batch mixing an already-current derived event with a genuinely new one is a SHORT
/// WRITE, and the shared authority must fold ONLY what was written, at the position the
/// store issued for it. The arithmetic this criterion deletes (`base = last + 1 - n`)
/// would fold BOTH events and stamp the suppressed one at a position that belongs to
/// another event entirely - and the graph's applied ledger is keyed BY position with
/// `INSERT OR IGNORE`, so that location is then marked applied forever and the genuine
/// event recorded there is swallowed with no recovery.
#[test]
fn a_partially_suppressed_append_folds_only_what_the_store_wrote_at_the_position_it_issued() {
    let store = guarded();
    let cap = CapturingProjection::default();

    let h1 = batch("src/a.rs", "h1");
    append_and_fold_batch(&store, Some(&cap as &dyn Projection), "run", &h1)
        .expect("the first append succeeds");
    assert_eq!(
        cap.folded(),
        held_positions(&store, "run"),
        "a first-seen generation folds whole, at the store's own positions"
    );

    // The short write: h1's first event is still this file's latest recorded generation,
    // so it is a storage no-op; the domain event beside it is genuinely new.
    let fresh = Event::new(TYPE_REVIEW_FINDING, b"a finding".to_vec());
    let mixed = vec![h1[0].clone(), fresh];
    let appended = append_and_fold_batch(&store, Some(&cap as &dyn Projection), "run", &mixed)
        .expect("a partially suppressed append is not an error");

    assert_eq!(
        appended.handed(),
        2,
        "the report names every event handed in, suppressed ones included"
    );
    assert_eq!(
        appended.written(),
        1,
        "exactly one of the two events was written"
    );
    assert!(
        appended.placements()[0].is_none(),
        "the already-current derived event is the suppressed slot"
    );

    let held = held_positions(&store, "run");
    assert_eq!(
        held.len(),
        3,
        "the log grew by exactly the one event written"
    );
    let written_at = held[2];
    assert_eq!(
        appended.placements()[1],
        Some(written_at),
        "the report names the position the store actually holds the written event at"
    );
    assert_eq!(
        cap.folded(),
        held,
        "the fold covers exactly the events the log holds, each at its own position - \
         never a suppressed event, and never a position derived by arithmetic"
    );
    assert_eq!(
        cap.batch_calls(),
        2,
        "one fold per append, and the short write folded through apply_batch like any other"
    );
}

/// An append that writes NOTHING folds nothing - and does not fold an EMPTY batch
/// either. The distinction matters: a projection handed an empty batch still opens a
/// transaction, and the criterion's no-op has to cost nothing above the port as well as
/// at it.
#[test]
fn a_fully_suppressed_append_folds_nothing_and_reports_an_absence() {
    let store = guarded();
    let cap = CapturingProjection::default();
    let h1 = batch("src/a.rs", "h1");

    append_and_fold_batch(&store, Some(&cap as &dyn Projection), "run", &h1).unwrap();
    let after_first = cap.folded();
    assert_eq!(after_first.len(), 2);

    let again = append_and_fold_batch(&store, Some(&cap as &dyn Projection), "run", &h1)
        .expect("re-offering a still-current generation is a no-op, not an error");

    assert_eq!(again.handed(), 2, "both events are still accounted for");
    assert_eq!(again.written(), 0, "neither was written");
    assert_eq!(
        again.last(),
        None,
        "an append that wrote nothing reports an absence, never a fabricated position 0"
    );
    assert_eq!(
        again.placed().count(),
        0,
        "there is nothing to zip against the batch"
    );
    assert_eq!(
        cap.folded(),
        after_first,
        "nothing new was folded, so the projection saw no second copy of the batch"
    );
    assert_eq!(
        cap.batch_calls(),
        1,
        "a fold with nothing to fold is not made at all - not even as an empty batch"
    );
    assert_eq!(
        held_positions(&store, "run").len(),
        2,
        "the log did not grow"
    );
}

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
/// contract between the layer that owns the key format and the layer that enforces the
/// guard. The TYPE test is the one that keeps a domain event from ever being dropped,
/// so it must be an EXACT match and never a prefix or a case-folded one, and the split
/// must be exactly what the caller injected - the store may add no interpretation.
#[test]
fn the_content_identity_policy_answers_exactly_what_it_was_configured_with() {
    let identity = ContentIdentity::new("replay_key", ["Alpha", "Beta"], pipe_subject_of);

    assert_eq!(identity.meta_key(), "replay_key");
    assert_eq!(identity.types(), ["Alpha".to_string(), "Beta".to_string()]);

    assert!(identity.covers("Alpha"));
    assert!(identity.covers("Beta"));
    for foreign in ["alpha", "ALPHA", "Alph", "AlphaExtra", "", "Gamma"] {
        assert!(
            !identity.covers(foreign),
            "{foreign:?} is not a configured type, so it can never be suppressed"
        );
    }

    assert_eq!(
        identity.subject_of("src/a.rs|h1"),
        Some(("src/a.rs|", "h1"))
    );
    assert_eq!(
        identity.subject_of("no-separator"),
        None,
        "a key the injected split does not recognise names no generation"
    );

    // The policy is a value the composition root may clone into several stores.
    let cloned = identity.clone();
    assert!(cloned.covers("Alpha"));
    assert_eq!(cloned.subject_of("s|g"), Some(("s|", "g")));
}

/// The `Debug` impl is a production trait impl on a type holding a FUNCTION POINTER.
/// It has to render the configuration a reader can act on and stop there: a pointer
/// address is noise that changes between runs and makes a diff of two debug renderings
/// spuriously unequal.
#[test]
fn the_policys_debug_rendering_shows_its_configuration_and_not_its_split() {
    let identity = ContentIdentity::new("replay_key", ["Alpha"], pipe_subject_of);
    let rendered = format!("{identity:?}");

    assert!(rendered.contains("ContentIdentity"), "{rendered}");
    assert!(rendered.contains("replay_key"), "{rendered}");
    assert!(rendered.contains("Alpha"), "{rendered}");
    assert!(
        rendered.contains(".."),
        "the rendering is non-exhaustive, so the split fn is named as omitted rather than \
         printed as an address: {rendered}"
    );
    assert!(
        !rendered.contains("0x"),
        "no pointer address leaks into the rendering: {rendered}"
    );
    assert_eq!(
        rendered,
        format!("{:?}", identity.clone()),
        "two renderings of the same configuration are identical"
    );
}

// ---------------------------------------------------------------------------
// The guard, driven at the store port as an external consumer
// ---------------------------------------------------------------------------

/// The store owns NO key format. The guard is driven here with a key shape this project
/// never mints (`<subject>|<generation>`) and a type set of the caller's own choosing,
/// and it must behave identically: suppress a still-current generation, append a new
/// one, append a reverted one, and never touch a type outside the configured set.
///
/// This is what a policy-shaped guard buys, and it is invisible to any test that
/// configures the one policy the project happens to use.
#[test]
fn the_store_owns_no_key_format_so_a_foreign_policy_guards_just_as_well() {
    let store = Store::open(":memory:")
        .unwrap()
        .with_content_identity(ContentIdentity::new(
            "widget_key",
            ["Widget"],
            pipe_subject_of,
        ));
    let widget = |key: &str| Event::new("Widget", b"w".to_vec()).with_meta("widget_key", key);
    let rows = |s: &Store| s.read_stream("run", 0, Direction::Forward).unwrap().len();

    let g1 = vec![widget("panel|g1")];
    assert_eq!(
        store
            .append("run", ExpectedRevision::Any, &g1)
            .unwrap()
            .written(),
        1,
        "a first-seen generation is written"
    );
    assert_eq!(
        store
            .append("run", ExpectedRevision::Any, &g1)
            .unwrap()
            .written(),
        0,
        "the still-current generation is a storage no-op under a foreign key shape too"
    );
    let g2 = vec![widget("panel|g2")];
    assert_eq!(
        store
            .append("run", ExpectedRevision::Any, &g2)
            .unwrap()
            .written(),
        1,
        "a new generation appends"
    );
    assert_eq!(
        store
            .append("run", ExpectedRevision::Any, &g1)
            .unwrap()
            .written(),
        1,
        "a generation the subject has moved past is a CHANGE and must append"
    );

    // A key the injected split does not recognise names no generation and is never
    // suppressed, however many times it is offered.
    let shapeless = vec![Event::new("Widget", b"w".to_vec()).with_meta("widget_key", "no-pipe")];
    for _ in 0..2 {
        assert_eq!(
            store
                .append("run", ExpectedRevision::Any, &shapeless)
                .unwrap()
                .written(),
            1,
            "an unrecognised key is passed through - the fail-safe direction"
        );
    }
    // A type outside the configured set keeps per-append identity, even carrying a key
    // that IS recorded and IS current.
    let foreign = vec![Event::new("Gadget", b"g".to_vec()).with_meta("widget_key", "panel|g1")];
    let before = rows(&store);
    for _ in 0..2 {
        assert_eq!(
            store
                .append("run", ExpectedRevision::Any, &foreign)
                .unwrap()
                .written(),
            1,
            "an unconfigured type is never suppressed, whatever its metadata says"
        );
    }
    assert_eq!(rows(&store), before + 2);
}

/// A SUPPRESSED EVENT CONSUMES NO PER-STREAM REVISION. The stream's revisions have to
/// stay contiguous across a no-op, because every reader that resumes from a revision
/// (`read_stream`, `subscribe_stream`, the optimistic-concurrency expectation) treats a
/// hole as a missing event: a projector replaying to revision 4 would wait forever for a
/// revision 2 that was never written.
///
/// The case that discriminates is the PARTIALLY suppressed append, and it is the one an
/// append-time cursor gets wrong: a whole-batch no-op writes nothing, so the next append
/// re-reads the stream's count and the hole never shows. Only a batch that suppresses
/// AND writes in the same append can leave one, so this drives that shape explicitly.
#[test]
fn a_suppressed_event_consumes_no_revision_so_the_stream_stays_contiguous() {
    let store = guarded();
    let h1 = batch("src/a.rs", "h1");
    let h2 = batch("src/a.rs", "h2");

    store.append("run", ExpectedRevision::Any, &h1).unwrap();
    store.append("run", ExpectedRevision::Any, &h1).unwrap(); // the whole-batch no-op
    store.append("run", ExpectedRevision::Any, &h2).unwrap();

    // The discriminating shape: h2 is still this file's latest generation, so its first
    // event is suppressed while the domain event beside it is written - one append that
    // both suppresses and writes.
    let mixed = vec![
        h2[0].clone(),
        Event::new(TYPE_REVIEW_FINDING, b"a finding".to_vec()),
    ];
    let appended = store.append("run", ExpectedRevision::Any, &mixed).unwrap();
    assert_eq!(
        appended.placements().iter().filter(|p| p.is_none()).count(),
        1,
        "the fixture really did suppress one event of this append"
    );
    assert_eq!(appended.written(), 1, "and really did write the other");

    let held = store.read_stream("run", 0, Direction::Forward).unwrap();
    assert_eq!(held.len(), 5, "two batches and one domain event landed");
    assert_eq!(
        held.iter().map(|e| e.revision).collect::<Vec<_>>(),
        vec![0, 1, 2, 3, 4],
        "the stream advanced by exactly the events WRITTEN, with no hole where a \
         suppressed event sat - not even inside an append that wrote beside it"
    );
    let positions: Vec<Position> = held.iter().map(|e| e.position).collect();
    assert!(
        positions.windows(2).all(|w| w[0] < w[1]),
        "global positions stay strictly increasing across a suppression: {positions:?}"
    );

    // The expectation a concurrent writer pins is the revision the stream actually
    // reached, so an exact-revision append still succeeds after a no-op.
    store
        .append("run", ExpectedRevision::Exact(4), &batch("src/b.rs", "hb"))
        .expect("the stream's revision is what the writes left it at, not what was handed in");
}

/// The guard is PROJECT-SCOPED through the decorator that gives each project its own
/// stream prefix - the production shape, which no unit test of the raw store exercises.
/// A content key names a RELATIVE path, so two projects sharing one backend mint
/// IDENTICAL keys for a shared file; an unscoped probe would read the second project's
/// genuinely-new fact as already recorded and drop it.
#[test]
fn two_projects_sharing_a_content_key_never_suppress_each_other_through_the_decorator() {
    let backend = guarded();
    let alpha = Namespaced::new(&backend, "alpha");
    let beta = Namespaced::new(&backend, "beta");
    let shared = batch("src/shared.rs", "same-hash");

    let a1 = alpha.append("run", ExpectedRevision::Any, &shared).unwrap();
    assert_eq!(a1.written(), 2, "the first project records its batch");
    assert_eq!(
        a1.handed(),
        2,
        "the decorator passes the report through with one slot per handed event"
    );

    let b1 = beta.append("run", ExpectedRevision::Any, &shared).unwrap();
    assert_eq!(
        b1.written(),
        2,
        "the SAME key under another project is a genuinely new fact and must append"
    );

    // ...and within one project the guard still suppresses.
    let a2 = alpha.append("run", ExpectedRevision::Any, &shared).unwrap();
    assert_eq!(
        a2.written(),
        0,
        "the still-current generation is a no-op inside its own project"
    );
    assert_eq!(a2.handed(), 2);

    assert_eq!(
        alpha
            .read_stream("run", 0, Direction::Forward)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        beta.read_stream("run", 0, Direction::Forward)
            .unwrap()
            .len(),
        2
    );
}

/// A store carrying a configured guard still owes every promise the port makes, over a
/// log that HAS suppressed something. The backend-agnostic contract suite is in-crate
/// and builds an unconfigured store, so nothing there covers a log with a no-op in its
/// history: the read conventions (a backward read is the forward set reversed; a
/// `read_all` resume from a position is inclusive of it) are exactly what a projection
/// rebuild and a subscription resume depend on, and a guard that perturbed them would
/// corrupt a rebuild rather than merely bound the log.
#[test]
fn the_ports_read_conventions_survive_a_suppression() {
    let store = guarded();
    let h1 = batch("src/a.rs", "h1");
    store.append("run", ExpectedRevision::Any, &h1).unwrap();
    store.append("run", ExpectedRevision::Any, &h1).unwrap(); // the no-op
    store
        .append("run", ExpectedRevision::Any, &batch("src/a.rs", "h2"))
        .unwrap();

    let forward = store.read_stream("run", 0, Direction::Forward).unwrap();
    let backward = store.read_stream("run", 0, Direction::Backward).unwrap();
    assert_eq!(forward.len(), 4);
    assert_eq!(
        forward.iter().map(|e| e.id.clone()).collect::<Vec<_>>(),
        backward
            .iter()
            .rev()
            .map(|e| e.id.clone())
            .collect::<Vec<_>>(),
        "a backward read returns the same set as a forward read, reversed"
    );

    let filter = Filter {
        stream_prefix: Some("run".to_string()),
    };
    let all = store.read_all(0, Direction::Forward, &filter).unwrap();
    assert_eq!(
        all.iter().map(|e| e.position).collect::<Vec<_>>(),
        forward.iter().map(|e| e.position).collect::<Vec<_>>(),
        "the global read agrees with the stream read across the no-op"
    );

    let third = forward[2].position;
    let resumed = store.read_all(third, Direction::Forward, &filter).unwrap();
    assert_eq!(
        resumed.iter().map(|e| e.position).collect::<Vec<_>>(),
        vec![forward[3].position],
        "a global resume stays EXCLUSIVE of its position and returns exactly the tail after it, \
         suppression history notwithstanding - a subscription resuming here must neither replay \
         nor skip"
    );
}

/// The one PERSISTED artifact this criterion adds is the lazily created content-key
/// index, and its lifecycle is only observable from outside the process that writes it -
/// which is the point: another process (a `rigger step`, a cold graph build, an operator
/// with a sqlite shell) sees the same file. It must not exist before an append that
/// could actually be suppressed, and it must exist after one, so a store that is only
/// ever read or only ever handed uncovered events writes no schema at all.
#[test]
fn the_content_key_index_is_built_lazily_by_the_first_append_that_could_be_suppressed() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let path = dir.path().join("events.db");
    let path_str = path.to_str().expect("a utf-8 path").to_string();

    // Asked of the COMMITTED DEFINITION, not of a name: the index the probes need is the
    // one built on THIS policy's metadata key, and an artifact built on any other answers
    // none of the guard's questions. An independent connection reads it, so what is
    // asserted is what the database holds rather than what a handle believes.
    let index_exists = || {
        let conn = rusqlite::Connection::open(&path_str).expect("an independent reader opens");
        let defs: Vec<String> = conn
            .prepare(
                "SELECT sql FROM sqlite_master \
                 WHERE type = 'index' AND tbl_name = 'events' AND sql IS NOT NULL",
            )
            .and_then(|mut s| {
                s.query_map([], |r| r.get::<_, String>(0))
                    .and_then(|rows| rows.collect())
            })
            .expect("sqlite_master is readable");
        let built: Vec<&String> = defs
            .iter()
            .filter(|sql| sql.contains(META_REPLAY_KEY) && sql.contains("json_extract"))
            .collect();
        assert!(
            built.len() <= 1,
            "one policy needs exactly one artifact: {built:?}"
        );
        built.len() == 1
    };

    let store = Store::open(&path_str)
        .unwrap()
        .with_content_identity(project_policy());
    assert!(
        !index_exists(),
        "configuring the guard touches no connection and writes no schema"
    );

    // An event of an uncovered type reaches no probe, so it builds nothing.
    store
        .append(
            "run",
            ExpectedRevision::Any,
            &[Event::new(TYPE_REVIEW_FINDING, b"f".to_vec())],
        )
        .unwrap();
    assert!(
        !index_exists(),
        "an append with nothing suppressible in it builds no index"
    );

    store
        .append("run", ExpectedRevision::Any, &batch("src/a.rs", "h1"))
        .unwrap();
    assert!(
        index_exists(),
        "the first append carrying a covered, keyed event builds the index the probes seek"
    );

    // A fresh handle on the SAME file finds the index already there and still answers
    // the guard's question correctly - the recorded state is the LOG'S, not a handle's.
    let fresh = Store::open(&path_str)
        .unwrap()
        .with_content_identity(project_policy());
    assert_eq!(
        fresh
            .append("run", ExpectedRevision::Any, &batch("src/a.rs", "h1"))
            .unwrap()
            .written(),
        0,
        "a second process suppresses on its first append, against the log the file carries"
    );
}

// ---------------------------------------------------------------------------
// The seams that now have to express "the store wrote nothing"
// ---------------------------------------------------------------------------

/// The emit seam, on both answers. On a real store it reports the position it wrote and
/// folds there; on a port that wrote nothing it FAILS and folds nowhere.
/// The second half is the one that matters: the applied ledger is keyed by position with
/// `INSERT OR IGNORE`, so a fold at a fabricated `0` marks that location applied forever
/// and swallows the genuine event recorded there.
#[test]
fn the_emit_seam_reports_what_the_store_wrote_and_folds_only_there() {
    let args = serde_json::json!({
        "type": TYPE_DECISION_MADE,
        "data": {"id": "d1", "summary": "a decision"},
    });

    let store = guarded();
    let cap = CapturingProjection::default();
    let pos = rigger::mcpserver::emit_event(&store, "run", Some(&cap as &dyn Projection), &args)
        .expect("the emit succeeds");
    let held = held_positions(&store, "run");
    assert_eq!(
        Some(pos),
        held.last().copied(),
        "the seam reports the position the store issued"
    );
    assert_eq!(
        cap.folded(),
        held,
        "the event folds at that same position, and nowhere else"
    );

    let silent = PortDouble::new(vec![None]);
    let cap2 = CapturingProjection::default();
    let message =
        rigger::mcpserver::emit_event(&silent, "run", Some(&cap2 as &dyn Projection), &args)
            .expect_err("a decision the store did not write was not emitted");
    assert!(
        message.contains("nothing"),
        "the seam reports the loss rather than a fabricated position: {message}"
    );
    assert!(
        cap2.folded().is_empty(),
        "nothing was written, so nothing folds - in particular nothing at position 0"
    );
}

/// THE FAIL-SAFE DIRECTION AT THE RUN-LIFECYCLE SEAMS. A configured guard must be
/// invisible to every stream that carries no content identity: the TYPE test is asked
/// FIRST, so a spawn result or a progress report - which carry no configured type and no
/// content key at all - append per append, even when two of them are byte-identical.
///
/// This is a cross-module integration seam (`spawn` and `progress` over `eventstore`)
/// and it is where the new "wrote nothing" error path lives; a guard that reached these
/// streams would turn a lost self-report into a silent success, which is exactly the
/// failure the port's honesty obligation exists to make impossible.
#[test]
fn a_configured_guard_never_reaches_the_spawn_and_progress_seams() {
    let store = guarded();

    let first = rigger::progress::record(&store, "run-1", "u1/impl#0", "the same line")
        .expect("recording progress succeeds");
    let second = rigger::progress::record(&store, "run-1", "u1/impl#0", "the same line")
        .expect("recording the identical line again succeeds");
    assert!(
        first < second,
        "two identical progress lines are two facts, at two positions: {first} then {second}"
    );

    let result = rigger::spawn::SpawnResult::ok("u1/impl#0", "done");
    let a = rigger::spawn::record_result(&store, &result).expect("recording a result succeeds");
    let b = rigger::spawn::record_result(&store, &result).expect("recording it again succeeds");
    assert!(
        a < b,
        "a re-recorded spawn result is a second fact, never a suppressed one: {a} then {b}"
    );
}

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

    let err = rigger::spawn::record_result(&silent, &result)
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
    let request = rigger::spawn::SpawnRequest::new("u1", "build", "impl", 0, "do the thing");
    let silent = PortDouble::new(vec![None]);

    let err = rigger::spawn::park_in_run(&silent, &request, "run-1")
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
    let outcome = rigger::spawn::record_result_if_absent(&quiet, &result);
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
    let message = rigger::run::start_fresh(&silent, &criteria, "hash-A", "base-sha")
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
    let message =
        rigger::run::ensure_started_pinned(&silent, &criteria, "hash-A", false, "base-sha")
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
    rigger::run::start_fresh(&live, &criteria, "hash-A", "base-sha").expect("a real run mints");
    let recorded = live
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .expect("the boundary reads back");
    assert_eq!(recorded.len(), 1, "the fixture is one real RunStarted");

    let drifted = PortDouble::over_a_stream(recorded, vec![None]);
    let message =
        rigger::run::ensure_started_pinned(&drifted, &criteria, "hash-B", true, "base-sha")
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
    let outcome = rigger::canary::run_canary(
        &silent,
        &NeverSpawns,
        &rigger::config::Config::default(),
        &panel,
        &[],
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

/// THE GUARD MEETS A SINGLE-EVENT APPEND - the one composition the shipped wiring cannot
/// reach yet and the composition root that configures a policy will.
///
/// Every seam that appends exactly one event now reads its report through one authority,
/// and on an unguarded store the only thing an absence there can mean is a lost write. A
/// store carrying a content-identity policy has a second way to write nothing, and it is
/// not a failure: the guard suppresses per EVENT, not per batch, so a lone event that is
/// already current is a legitimate absence.
///
/// The SAFETY half must hold under both, and is what this pins: no position is handed
/// back for an event the store did not write - above all not the earlier event's, which is
/// the one fabrication a guarded store could plausibly make - and the log still holds
/// exactly one copy. Only a periphery test can see this at all: the guard is `eventstore`,
/// the accessor is the port, and the seams that ask it are five other modules, so no
/// single module's tests span it.
///
/// What the caller is TOLD is recorded here rather than argued. Today a suppression on a
/// one-event append reaches it as the same failure a lost write does, which is right for
/// every seam wired today - each records a run-lifecycle type no policy reaches - and is
/// the obligation whoever configures `with_content_identity` over a stream a single-event
/// seam writes to inherits. Stated in a test that reds the moment the answer changes,
/// rather than in prose nothing checks.
#[test]
fn a_guarded_store_answers_a_single_event_seam_with_no_position_it_did_not_issue() {
    let store = guarded();
    let event = keyed(TYPE_CODE_ENTITY_EXTRACTED, "gc/src/a.rs@h1#0");

    let first = store
        .append(
            "guarded-one",
            ExpectedRevision::Any,
            std::slice::from_ref(&event),
        )
        .expect("the first append succeeds")
        .one("the extraction of src/a.rs")
        .expect("the store wrote it and can say where");

    let again = store
        .append(
            "guarded-one",
            ExpectedRevision::Any,
            std::slice::from_ref(&event),
        )
        .expect("a suppressed append is not an append error");
    assert_eq!(
        again.written(),
        0,
        "the guard recognised the lone event as already recorded"
    );
    let message = again
        .one("the extraction of src/a.rs")
        .expect_err("a seam whose one event was suppressed is handed no position at all")
        .to_string();
    assert!(
        message.contains("the extraction of src/a.rs"),
        "and the answer names what the caller was recording, so an operator can tell WHICH \
         write has no position: {message}"
    );

    assert_eq!(
        held_positions(&store, "guarded-one"),
        vec![first],
        "the log holds exactly one copy, at the position the first append cited - the guard \
         suppressed the duplicate and invented nothing to replace it"
    );
}

// ---------------------------------------------------------------------------
// The operator's surface: the two commands that now print what the store wrote
// ---------------------------------------------------------------------------

/// A throwaway project the compiled binary will accept: its own git repo (so the store's
/// project identity resolves exactly as a real project's does), a pinned `project.id` so
/// the stream this test reads back is the stream the binary wrote to whatever the temp
/// directory is called, and an INITIALIZED event log - the binary refuses to fabricate one.
fn cli_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temp project");
    let root = dir.path();
    let _ = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(root)
        .status();
    let rigger_dir = root.join(".rigger");
    std::fs::create_dir_all(&rigger_dir).expect("create .rigger");
    std::fs::write(rigger_dir.join("project.id"), "u4-cli-surface").expect("pin the identity");
    // Opening the store creates the schema the binary then appends to.
    Store::open(
        rigger_dir
            .join("events.db")
            .to_str()
            .expect("a utf-8 store path"),
    )
    .expect("the event log initializes");
    dir
}

// The compiled `rigger` binary under test is located at RUNTIME by the shared authority in
// `tests/common`: a path baked in at compile time goes stale the moment the target dir moves,
// and every suite that spawns the product then dies with a bare NotFound.
mod common;

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

/// A PORT MUST NOT BE SATISFIABLE BY A VALUE IT CANNOT HONOR.
///
/// The guard needs to know where in a content key the generation ENDS - that offset is
/// what lets it step past a whole generation in one index seek instead of walking a
/// file's every recorded event. A policy that handed back two strings could satisfy the
/// signature while pointing nowhere into the key it was given, and then the guard could
/// not locate anything, would quietly stop suppressing, and would report exactly what an
/// unguarded store reports. That failure is INVISIBLE from outside: `written == handed`
/// is what a working store says when there is nothing to suppress.
///
/// So the port hands back RANGES, which cannot point at another allocation, and it
/// VALIDATES them: the subject must start the key, the generation must lie within it at
/// or after the subject, and every boundary must be a character boundary. A policy that
/// breaks any of those names no generation at all - which appends. A composition root
/// that gets this wrong therefore degrades to an unguarded store, loudly enough to see
/// in the row count, and can never drop a fact or panic a writer on a multi-byte path.
#[test]
fn a_policy_whose_ranges_do_not_describe_the_key_guards_nothing_and_never_panics() {
    // The subject does not START the key, so "every key naming this subject begins with
    // it" - the property the range seek rests on - is false.
    fn detached_subject(key: &str) -> Option<(Range<usize>, Range<usize>)> {
        Some((1..4, 4..key.len()))
    }
    // The generation runs past the end of the key it was handed.
    fn past_the_end(key: &str) -> Option<(Range<usize>, Range<usize>)> {
        Some((0..key.len(), key.len()..key.len() + 8))
    }
    // An inverted range: a slice that cannot be taken.
    fn inverted(key: &str) -> Option<(Range<usize>, Range<usize>)> {
        Some((0..key.len(), key.len()..0))
    }
    // A boundary in the MIDDLE of a multi-byte character - the one that would panic.
    fn mid_character(key: &str) -> Option<(Range<usize>, Range<usize>)> {
        Some((0..4, 4..key.len()))
    }

    let key = "gc/é.rs@h1#0";
    assert!(!key.is_char_boundary(4), "the fixture key is multi-byte");

    for (named, split) in [
        (
            "a subject that does not start the key",
            detached_subject as fn(&str) -> Option<(Range<usize>, Range<usize>)>,
        ),
        ("a generation past the end of the key", past_the_end),
        ("an inverted range", inverted),
        ("a boundary inside a character", mid_character),
    ] {
        let identity = ContentIdentity::new(META_REPLAY_KEY, DERIVED_INDEX_TYPES, split);
        assert_eq!(
            identity.split_of(key),
            None,
            "{named} does not describe a split of the key, so it names no generation"
        );
        assert_eq!(identity.subject_of(key), None, "{named}");

        // And the store built on it DEGRADES to appending, twice over, rather than
        // suppressing on a split it cannot trust.
        let store = Store::open(":memory:")
            .unwrap()
            .with_content_identity(identity);
        let events = vec![keyed(TYPE_CODE_ENTITY_EXTRACTED, key)];
        for round in 0..2 {
            assert_eq!(
                store
                    .append("run", ExpectedRevision::Any, &events)
                    .unwrap()
                    .written(),
                1,
                "{named}, round {round}: an unusable split appends - it never drops, and \
                 never panics on a multi-byte key"
            );
        }
    }

    // THE CONTROL, on the same shape: a policy that does describe its key suppresses.
    let store = Store::open(":memory:")
        .unwrap()
        .with_content_identity(project_policy());
    let events = vec![keyed(TYPE_CODE_ENTITY_EXTRACTED, "gc/src/a.rs@h1#0")];
    assert_eq!(
        store
            .append("run", ExpectedRevision::Any, &events)
            .unwrap()
            .written(),
        1
    );
    assert_eq!(
        store
            .append("run", ExpectedRevision::Any, &events)
            .unwrap()
            .written(),
        0,
        "the difference between the arms above and this one is the POLICY, not the fixture"
    );
}

/// THE GUARD ON A LOG THAT IS ALREADY BIG, WITH A CONTROL BESIDE IT.
///
/// Every other fixture in this suite is a handful of rows in a fresh `:memory:` store,
/// and a handful of rows cannot tell a guard that is working from a guard that has
/// switched itself off: at four rows both answers look plausible, and a probe that
/// degraded into a full table walk still returns the right verdict. This one runs on a
/// FILE-BACKED store carrying an established history - hundreds of files, several
/// content generations deep, with domain events interleaved - and it pins the guard's
/// value as a DIFFERENCE:
///
///  - the control, an unguarded handle on that same log, re-appends the whole re-derived
///    index, which is precisely the unbounded growth this spec exists to stop;
///  - the treatment, a guarded handle on it, writes NOTHING;
///  - and the three things that must still land, land: a file whose content changed, a
///    file REVERTED to a generation it has moved past, and a domain event.
#[test]
fn on_an_established_log_the_guard_is_the_only_thing_that_stops_the_duplication() {
    const FILES: usize = 120;
    const PER_BATCH: usize = 4;

    let dir = tempfile::tempdir().expect("a temp dir");
    let path = dir.path().join("events.db");
    let path = path.to_str().expect("a utf-8 path").to_string();

    let file_batch = |file: usize, hash: &str| -> Vec<Event> {
        (0..PER_BATCH)
            .map(|i| {
                keyed(
                    if i % 2 == 0 {
                        TYPE_CODE_ENTITY_EXTRACTED
                    } else {
                        TYPE_EDGE_INFERRED
                    },
                    &format!("gc/src/f{file:04}.rs@{hash}#{i}"),
                )
            })
            .collect()
    };

    // An established log: every file recorded at h1, a third of them changed to h2, a
    // ninth changed again to h3, with domain events interleaved throughout.
    let seed = Store::open(&path).expect("a file-backed store opens");
    for file in 0..FILES {
        seed.append("proj", ExpectedRevision::Any, &file_batch(file, "h1"))
            .unwrap();
        if file % 3 == 0 {
            seed.append("proj", ExpectedRevision::Any, &file_batch(file, "h2"))
                .unwrap();
        }
        if file % 9 == 0 {
            seed.append("proj", ExpectedRevision::Any, &file_batch(file, "h3"))
                .unwrap();
        }
        if file % 10 == 0 {
            seed.append(
                "proj",
                ExpectedRevision::Any,
                &[Event::new(TYPE_REVIEW_FINDING, b"a finding".to_vec())],
            )
            .unwrap();
        }
    }
    let established = held_positions(&seed, "proj").len();
    assert!(
        established > 600,
        "the fixture is an ESTABLISHED log, not a handful of rows: {established} events"
    );
    drop(seed);

    // The current generation of every file, which is what a re-derivation over an
    // unchanged tree hands the store again on the next run.
    let rederived: Vec<Event> = (0..FILES)
        .flat_map(|file| {
            let hash = if file % 9 == 0 {
                "h3"
            } else if file % 3 == 0 {
                "h2"
            } else {
                "h1"
            };
            file_batch(file, hash)
        })
        .collect();

    // THE CONTROL. An unguarded handle on this same log writes every one of them.
    let unguarded = Store::open(&path).unwrap();
    assert_eq!(
        unguarded
            .append("proj", ExpectedRevision::Any, &rederived)
            .unwrap()
            .written(),
        rederived.len(),
        "with no guard the whole re-derived index lands again - the growth this spec exists \
         to stop"
    );
    let after_control = held_positions(&unguarded, "proj").len();
    assert_eq!(after_control, established + rederived.len());
    drop(unguarded);

    // THE TREATMENT. A guarded handle on the same log writes nothing at all.
    let guarded = Store::open(&path)
        .unwrap()
        .with_content_identity(project_policy());
    let appended = guarded
        .append("proj", ExpectedRevision::Any, &rederived)
        .unwrap();
    assert_eq!(
        appended.written(),
        0,
        "every key handed in is its file's CURRENT generation and already recorded"
    );
    assert_eq!(appended.handed(), rederived.len());
    assert_eq!(
        appended.last(),
        None,
        "an append that wrote nothing says so"
    );
    assert_eq!(
        held_positions(&guarded, "proj").len(),
        after_control,
        "and the log did not grow by one row"
    );

    // A file whose content CHANGED still lands, whole.
    let changed = file_batch(7, "h4");
    assert_eq!(
        guarded
            .append("proj", ExpectedRevision::Any, &changed)
            .unwrap()
            .written(),
        changed.len(),
        "a new generation is not redundant"
    );

    // A REVERT - file 7 driven back to h1, a generation it has moved past - is a CHANGE.
    let reverted = file_batch(7, "h1");
    assert_eq!(
        guarded
            .append("proj", ExpectedRevision::Any, &reverted)
            .unwrap()
            .written(),
        reverted.len(),
        "an ever-recorded test would swallow this and strand the projection on h4 forever"
    );

    // And a domain event still appends, on a log where the guard is demonstrably ON.
    assert_eq!(
        guarded
            .append(
                "proj",
                ExpectedRevision::Any,
                &[Event::new(TYPE_REVIEW_FINDING, b"a finding".to_vec())],
            )
            .unwrap()
            .written(),
        1,
        "identical domain events are two facts, whatever the guard is doing"
    );
}

// ---------------------------------------------------------------------------
// The degradation mark: what a guard that has stopped defending says, and where
// ---------------------------------------------------------------------------

/// The NAME of the content-key index this project's policy needs, read off the database
/// rather than derived in the test. The store owns that name; a test that recomputed it
/// would pin the derivation instead of the artifact, and would keep passing after the
/// derivation and the store drifted apart.
fn content_key_index_name(path: &str) -> String {
    let conn = rusqlite::Connection::open(path).expect("an independent reader opens");
    let mut stmt = conn
        .prepare(
            "SELECT name, sql FROM sqlite_master \
             WHERE type = 'index' AND tbl_name = 'events' AND sql IS NOT NULL",
        )
        .expect("sqlite_master is readable");
    let mut named: Vec<String> = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .expect("sqlite_master is queryable")
        .map(|row| row.expect("a sqlite_master row"))
        .filter(|(_, sql)| sql.contains(META_REPLAY_KEY) && sql.contains("json_extract"))
        .map(|(name, _)| name)
        .collect();
    assert_eq!(
        named.len(),
        1,
        "one policy needs exactly one content-key artifact: {named:?}"
    );
    named.pop().expect("just asserted one")
}

/// Run maintenance SQL through an INDEPENDENT connection - the operator's shell, or
/// another process - so what the store meets is the database's real state and not
/// something a handle was told.
fn sql(path: &str, statements: &str) {
    rusqlite::Connection::open(path)
        .expect("an independent writer opens")
        .execute_batch(statements)
        .expect("the maintenance statements run");
}

/// The `meta` column exactly as the log holds it, in log order, for one stream - what an
/// operator with a SQL prompt sees, with no Rust type in the way.
fn stored_meta(path: &str, stream: &str) -> Vec<String> {
    let conn = rusqlite::Connection::open(path).expect("an independent reader opens");
    let mut stmt = conn
        .prepare("SELECT meta FROM events WHERE stream = ?1 ORDER BY position")
        .expect("the events table is readable");
    let rows: Vec<String> = stmt
        .query_map([stream], |r| r.get::<_, String>(0))
        .expect("the events table is queryable")
        .map(|row| row.expect("an events row"))
        .collect();
    rows
}

/// A GUARD THAT HAS STOPPED DEFENDING SAYS SO, AND AN OPERATOR CAN SEE IT.
///
/// The three constants this criterion adds ([`META_GUARD_DEGRADED`] and the two reasons)
/// are a public vocabulary and a new SERIALIZED form: a metadata pair persisted onto
/// every covered row an unjudging append writes. `src/eventstore/mod.rs` promises that
/// "any read of the store, and any operator with a SQL prompt" can see it, and that
/// promise is not a statement about one struct's method - it spans the port, the
/// project-scoping decorator every command actually composes, both read paths, and the
/// bytes in the file. No in-crate test can reach any of that: the store's own tests
/// drive a bare `:memory:` handle and read it back through one method on that same
/// handle.
///
/// The mark's whole value is that its PRESENCE is information, so this drives the guard
/// through both of its states on one log: off (every fact still lands, and every covered
/// row says why it was not judged) and back on (nothing is stamped, and the suppression
/// resumes). A mark that appeared on a healthy append would be noise an operator learns
/// to ignore.
#[test]
fn a_guard_that_stopped_defending_says_so_in_the_log_it_guards() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let path = dir.path().join("events.db");
    let path = path.to_str().expect("a utf-8 path").to_string();
    let scoped = "proj-alpha-run"; // what the decorator names the stream on disk

    // A HEALTHY log first, so the artifact exists to be taken away and the events the
    // degraded append re-offers are genuinely current.
    {
        let store = Store::open(&path)
            .unwrap()
            .with_content_identity(project_policy());
        let alpha = Namespaced::new(&store, "alpha");
        let port: &dyn EventStore = &alpha;
        port.append("run", ExpectedRevision::Any, &batch("src/a.rs", "h1"))
            .unwrap();
        assert_eq!(
            port.append("run", ExpectedRevision::Any, &batch("src/a.rs", "h1"))
                .unwrap()
                .written(),
            0,
            "the fixture starts with the guard demonstrably ON"
        );
    }

    // Take the artifact away in the one way a handle cannot paper over: occupy its name
    // with a table, so every build this policy attempts fails to commit.
    let index = content_key_index_name(&path);
    sql(
        &path,
        &format!("DROP INDEX {index}; CREATE TABLE {index}(blocker);"),
    );

    let store = Store::open(&path)
        .unwrap()
        .with_content_identity(project_policy());
    let alpha = Namespaced::new(&store, "alpha");
    let port: &dyn EventStore = &alpha;

    // Two covered events at a generation that IS recorded and IS current - a healthy
    // guard suppresses both - one of them carrying the caller's own metadata beside its
    // content key, plus a domain event carrying the same caller metadata.
    let handed = vec![
        Event::new(TYPE_CODE_ENTITY_EXTRACTED, b"payload".to_vec())
            .with_meta(META_REPLAY_KEY, "gc/src/a.rs@h1#0")
            .with_meta("courier", "u4"),
        keyed(TYPE_EDGE_INFERRED, "gc/src/a.rs@h1#1"),
        Event::new(TYPE_REVIEW_FINDING, b"a finding".to_vec()).with_meta("courier", "u4"),
    ];
    let appended = port.append("run", ExpectedRevision::Any, &handed).unwrap();
    assert_eq!(
        appended.written(),
        3,
        "no usable index, no suppression: the fail-safe direction is to WRITE, which is \
         exactly why the degradation has to be recorded - the log simply starts growing \
         again and nothing else says why"
    );

    let recorded = port.read_stream("run", 0, Direction::Forward).unwrap();
    assert_eq!(
        recorded.len(),
        5,
        "the seeded pair plus the three just written"
    );
    let written = &recorded[2..];
    for (i, event) in written.iter().take(2).enumerate() {
        assert_eq!(
            event.meta.get(META_GUARD_DEGRADED).map(String::as_str),
            Some(GUARD_DEGRADED_NO_INDEX),
            "covered event {i} was admitted by a guard that was not judging, and names WHICH \
             defence gave way - `no-index` and `generations-exceeded` ask for different remedies"
        );
    }
    assert_eq!(
        written[2].meta.get(META_GUARD_DEGRADED),
        None,
        "a domain event is never rewritten by a store that merely happened to be unhealthy \
         while it landed"
    );

    // The mark is ADDITIVE. It rides on events the append was already writing, so it may
    // not displace what the caller put there - least of all the content key the guard's
    // own index is built on.
    assert_eq!(
        written[0].meta.get("courier").map(String::as_str),
        Some("u4"),
        "the caller's own metadata survives verbatim"
    );
    assert_eq!(
        written[0].meta.get(META_REPLAY_KEY).map(String::as_str),
        Some("gc/src/a.rs@h1#0"),
        "and so does the content key, or the row stops being findable by the very index \
         whose absence is being reported"
    );
    assert_eq!(
        written[2].meta.get("courier").map(String::as_str),
        Some("u4"),
        "an unmarked domain event keeps its metadata too"
    );

    // THE OTHER READ PATH. `read_stream` and `read_all` are different statements, and a
    // subscription resume and a projection rebuild take the second one.
    let globally = port
        .read_all(0, Direction::Forward, &Filter::default())
        .unwrap();
    assert_eq!(
        globally
            .iter()
            .filter_map(|e| e.meta.get(META_GUARD_DEGRADED))
            .count(),
        2,
        "the global read surfaces the same two marks the stream read does"
    );

    // AND THE BYTES. The claim is that an operator with a SQL prompt sees it, which is a
    // claim about the file - read here through a connection that knows nothing about
    // this crate's types.
    let on_disk = stored_meta(&path, scoped);
    assert_eq!(on_disk.len(), 5);
    assert!(
        on_disk[2].contains(META_GUARD_DEGRADED) && on_disk[2].contains(GUARD_DEGRADED_NO_INDEX),
        "the reason is IN the row, not in a process that has since exited: {}",
        on_disk[2]
    );
    assert!(
        on_disk[2].contains("courier"),
        "beside the caller's own pairs, in one meta object: {}",
        on_disk[2]
    );
    assert!(
        !on_disk[4].contains(META_GUARD_DEGRADED),
        "and not on the domain row: {}",
        on_disk[4]
    );

    // GIVE THE ARTIFACT BACK. A guard that is judging stamps nothing, so the mark means
    // what it says, and the suppression it was not doing resumes.
    drop(alpha);
    drop(store);
    sql(&path, &format!("DROP TABLE {index};"));

    let healed = Store::open(&path)
        .unwrap()
        .with_content_identity(project_policy());
    let alpha = Namespaced::new(&healed, "alpha");
    let port: &dyn EventStore = &alpha;
    let fresh = port
        .append("run", ExpectedRevision::Any, &batch("src/b.rs", "h1"))
        .unwrap();
    assert_eq!(fresh.written(), 2, "a new subject's first generation lands");
    assert_eq!(
        port.append("run", ExpectedRevision::Any, &batch("src/b.rs", "h1"))
            .unwrap()
            .written(),
        0,
        "and the guard is genuinely back on - the rows the degraded append wrote did not \
         confuse it"
    );
    let after = port.read_stream("run", 0, Direction::Forward).unwrap();
    assert!(
        after[5..]
            .iter()
            .all(|e| !e.meta.contains_key(META_GUARD_DEGRADED)),
        "a healthy append stamps nothing: the mark's PRESENCE is the information"
    );
}

/// THE TWO OFF STATES ARE DIFFERENT FACTS, and only the reason on the row tells them
/// apart. `no-index` says an artifact is missing - rebuild it. `generations-exceeded`
/// says the artifact is there and one subject has recorded more generations than the
/// probe may step through - that subject needs compaction, and rebuilding the index
/// would change nothing. A single "the guard is unwell" mark would send an operator to
/// the wrong remedy, so this drives the second state through the same public port and
/// the same decorator as the first, and pins that a subject the walk CAN answer is still
/// guarded on the very same log.
#[test]
fn an_exhausted_generation_walk_names_a_different_defence_than_a_missing_index() {
    // The store's latest-generation walk has a step budget it keeps to itself. This
    // fixture has to exceed it, so it is deliberately far above any plausible bound; the
    // assertion below fails loudly if the budget ever grows past it, rather than quietly
    // ceasing to reach the state it exists to cover.
    const GENERATIONS: usize = 1500;

    let dir = tempfile::tempdir().expect("a temp dir");
    let path = dir.path().join("events.db");
    let path = path.to_str().expect("a utf-8 path").to_string();

    let store = Store::open(&path)
        .unwrap()
        .with_content_identity(project_policy());
    let alpha = Namespaced::new(&store, "alpha");
    let port: &dyn EventStore = &alpha;

    // One subject with more recorded generations than the walk may step through. They go
    // in as one append: every verdict is taken against the state the log was in when the
    // append started, so none of them is redundant and none of them walks.
    let history: Vec<Event> = (0..GENERATIONS)
        .map(|i| {
            keyed(
                TYPE_CODE_ENTITY_EXTRACTED,
                &format!("gc/src/a.rs@h{i:06}#0"),
            )
        })
        .collect();
    assert_eq!(
        port.append("run", ExpectedRevision::Any, &history)
            .unwrap()
            .written(),
        GENERATIONS,
        "every generation of the deep subject is a genuinely new fact"
    );

    // A shallow subject on the SAME log, so the two verdicts can be compared.
    port.append("run", ExpectedRevision::Any, &batch("src/b.rs", "h1"))
        .unwrap();

    // Re-offering a RECORDED but no-longer-current generation of the deep subject is
    // what sends the probe walking.
    let stale = vec![keyed(TYPE_CODE_ENTITY_EXTRACTED, "gc/src/a.rs@h000000#0")];
    let appended = port.append("run", ExpectedRevision::Any, &stale).unwrap();
    assert_eq!(
        appended.written(),
        1,
        "an undetermined walk never suppresses - it appends, which is the only safe \
         direction when the guard does not know what the subject is currently at"
    );

    let recorded = port.read_stream("run", 0, Direction::Forward).unwrap();
    let last = recorded.last().expect("the append landed");
    assert_eq!(
        last.meta.get(META_GUARD_DEGRADED).map(String::as_str),
        Some(GUARD_DEGRADED_UNDETERMINED),
        "the duplicate this let through is EXPLAINABLE rather than mysterious, and it names \
         the walk rather than the index - if this fails with `None`, the walk's step budget \
         has grown past this fixture's {GENERATIONS} generations and the fixture, not the \
         guard, is what needs raising"
    );

    // The degradation is one append's fact about one walk, not a latch on the store: the
    // shallow subject, whose current generation the walk answers in a step or two, is
    // still guarded on this same log and still stamps nothing.
    let shallow = port
        .append("run", ExpectedRevision::Any, &batch("src/b.rs", "h1"))
        .unwrap();
    assert_eq!(
        shallow.written(),
        0,
        "a subject the walk CAN answer keeps its guard while another subject is beyond it"
    );
    let after = port.read_stream("run", 0, Direction::Forward).unwrap();
    assert_eq!(
        after.len(),
        recorded.len(),
        "and that suppression wrote no row for a mark to ride on"
    );
}

// ---------------------------------------------------------------------------
// The policy is CONFIGURATION, so a log outlives the policy that guarded it
// ---------------------------------------------------------------------------

/// Every content-key artifact the FILE carries, as `(name, definition)` sorted by name,
/// read through an INDEPENDENT connection.
///
/// Deliberately not filtered by any one policy's metadata key: this is the question
/// "what does the database carry" rather than "is the artifact I expect present", which
/// is the only shape that can see an artifact left behind by a policy nobody configures
/// any more. `json_extract` is what makes a content-key index recognizable without
/// knowing whose key it reads.
fn content_key_artifacts(path: &str) -> Vec<(String, String)> {
    let conn = rusqlite::Connection::open(path).expect("an independent reader opens");
    let mut stmt = conn
        .prepare(
            "SELECT name, sql FROM sqlite_master \
             WHERE type = 'index' AND tbl_name = 'events' AND sql IS NOT NULL \
             ORDER BY name",
        )
        .expect("sqlite_master is readable");
    stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .expect("sqlite_master is queryable")
        .map(|row| row.expect("a sqlite_master row"))
        .filter(|(_, sql)| sql.contains("json_extract"))
        .collect()
}

/// A LOG THAT MEETS A NEW POLICY IS GUARDED BY THAT POLICY, AND STOPS CARRYING THE OLD
/// ONE'S ARTIFACT.
///
/// The guard's policy is injected at the composition root, and a composition root is a
/// thing that gets EDITED: the metadata key derived facts ride under is a code-owned
/// constant, so the day it is renamed, every existing database is a log built under one
/// policy that a new binary opens under another. That is not an exotic configuration, it
/// is what an upgrade IS, and nothing about it is visible from inside one store's tests:
/// the crate's own reconfiguration test drives one `:memory:` handle whose builder is
/// called twice and reads back index definitions, which is the artifact question, on the
/// composition (one connection, one process) that a deployment never has.
///
/// So this drives the deployment shape - two independently opened handles on ONE FILE,
/// the second one being what a redeployed binary is - and asks the three questions an
/// operator would:
///
///  - is the new policy's guard actually LIVE against a log full of another policy's
///    rows, or did it inherit an artifact that answers nothing it asks and quietly stop
///    suppressing (the silent degradation this index's name exists to prevent);
///  - does the file stop carrying the retired artifact, which SQLite would otherwise
///    maintain on every insert forever - on a criterion whose whole purpose is to BOUND
///    the store, one dead index per rename is the guard growing what it was built to
///    shrink;
///  - and are the facts recorded under the RETIRED key safe, which is the direction that
///    matters: they name no generation to the live policy, so they append. A store that
///    read them through the old key's eyes would drop facts on the day the policy
///    changed, which is the one failure this guard may never have.
#[test]
fn a_log_that_meets_a_new_policy_is_guarded_by_it_and_stops_carrying_the_retired_artifact() {
    // The key a previous build of this project would have carried its content keys
    // under. It is not `META_REPLAY_KEY`, which is the whole point.
    const RETIRED_KEY: &str = "content_key_v1";

    let dir = tempfile::tempdir().expect("a temp dir");
    let path = dir.path().join("events.db");
    let path = path.to_str().expect("a utf-8 path").to_string();

    let under_retired_key = |key: &str| {
        Event::new(TYPE_CODE_ENTITY_EXTRACTED, b"payload".to_vec()).with_meta(RETIRED_KEY, key)
    };
    let retired_generation = vec![under_retired_key("gc/src/a.rs@h1#0")];

    // ROUND ONE: the retired policy guards this log, and mints its own artifact doing it.
    {
        let store = Store::open(&path)
            .expect("a file-backed store opens")
            .with_content_identity(ContentIdentity::new(
                RETIRED_KEY,
                DERIVED_INDEX_TYPES,
                path_subject_of,
            ));
        assert_eq!(
            store
                .append("run", ExpectedRevision::Any, &retired_generation)
                .unwrap()
                .written(),
            1,
            "the first recording of a generation lands"
        );
        assert_eq!(
            store
                .append("run", ExpectedRevision::Any, &retired_generation)
                .unwrap()
                .written(),
            0,
            "and the retired policy really was guarding this log - without this, round two \
             proves nothing"
        );
    }
    let retired_artifacts = content_key_artifacts(&path);
    assert_eq!(
        retired_artifacts.len(),
        1,
        "the retired policy left exactly one artifact behind: {retired_artifacts:?}"
    );
    let (retired_name, retired_ddl) = retired_artifacts[0].clone();
    assert!(
        retired_ddl.contains(RETIRED_KEY),
        "and it reads the retired key, which nothing will carry again: {retired_ddl}"
    );

    // ROUND TWO: a FRESH handle on that same file under the CURRENT policy - a redeployed
    // binary opening the database it inherited.
    let store = Store::open(&path)
        .expect("the second handle opens the same file")
        .with_content_identity(project_policy());
    let current = batch("src/a.rs", "h1");
    assert_eq!(
        store
            .append("run", ExpectedRevision::Any, &current)
            .unwrap()
            .written(),
        current.len(),
        "the current policy's first batch is new to it, whatever the log holds under \
         another key"
    );
    assert_eq!(
        store
            .append("run", ExpectedRevision::Any, &current)
            .unwrap()
            .written(),
        0,
        "the new policy's guard is LIVE on an inherited log: it minted the artifact its \
         own probes seek rather than inheriting one that indexes a key nothing carries"
    );
    assert!(
        stored_meta(&path, "run")
            .iter()
            .all(|meta| !meta.contains(META_GUARD_DEGRADED)),
        "and it never had to announce a degradation to get there: a policy change is a \
         supported configuration, not an outage"
    );

    // The FILE now carries the live policy's artifact, and only it.
    let live_artifacts = content_key_artifacts(&path);
    assert_eq!(
        live_artifacts.len(),
        1,
        "one configured policy, one artifact: an index no policy uses is still maintained \
         on every insert and still occupies the file, so it is reclaimed rather than left \
         behind: {live_artifacts:?}"
    );
    let (live_name, live_ddl) = live_artifacts[0].clone();
    assert_ne!(
        live_name, retired_name,
        "the live artifact is not the retired one wearing a new policy's expectations"
    );
    assert!(
        live_ddl.contains(META_REPLAY_KEY),
        "it indexes the key the configured policy actually reads: {live_ddl}"
    );

    // THE DIRECTION THAT MATTERS. A fact carried under the retired key names no
    // generation to the live policy, so it appends - twice over, because a guard that
    // suppressed it would be dropping facts it cannot judge.
    for round in 0..2 {
        assert_eq!(
            store
                .append("run", ExpectedRevision::Any, &retired_generation)
                .unwrap()
                .written(),
            1,
            "round {round}: an event whose key the configured policy does not read is not \
             a duplicate, it is unjudgeable - and unjudgeable appends"
        );
    }
}

/// A GUARD MAY NOT WEAKEN THE CONCURRENCY CONTRACT IT SITS IN FRONT OF, and the append
/// that would be cheapest to shortcut is the one where it must not.
///
/// Optimistic concurrency is the port's promise to every writer that hands in an
/// expectation: the append happens against the revision the caller last read, or it
/// fails with what the revision actually is. Suppression sits UNDER that promise - it is
/// a decision about which rows to write, taken after the expectation has been settled -
/// and an all-suppressed append is exactly the shape where the two are easy to confuse.
/// It writes nothing, so an implementation that reasons "nothing to write, nothing to
/// conflict with" and answers early is both plausible and silently wrong: a writer whose
/// expectation is stale would be told its append succeeded against a stream that has
/// moved under it, which is the read-modify-write race the expectation exists to catch.
/// The mistake is invisible in the row counts, because there are none either way.
///
/// No test outside this crate references the port's conflict at all, and every in-crate
/// one drives an UNGUARDED store, so this pins the composition: a guarded store, a batch
/// whose every event the guard would suppress, and all three expectations.
#[test]
fn an_all_suppressed_append_still_answers_a_stale_expectation_with_a_conflict() {
    let store = guarded();
    let h1 = batch("src/a.rs", "h1");
    store
        .append("run", ExpectedRevision::Any, &h1)
        .expect("the first recording lands");
    let landed = store.read_stream("run", 0, Direction::Forward).unwrap();
    assert_eq!(landed.len(), h1.len(), "the fixture recorded its batch");

    // The stream is at revision 1. A writer pinning revision 0 read it before that batch
    // landed, and its append - every event of which the guard would suppress - must be
    // refused on the expectation, not accepted on the emptiness of its write.
    let refused = store
        .append("run", ExpectedRevision::Exact(0), &h1)
        .expect_err("a stale expectation is a conflict even when nothing would be written");
    match refused {
        StoreError::Conflict { actual, .. } => assert_eq!(
            actual,
            (h1.len() - 1) as Revision,
            "and the conflict reports the revision the stream is ACTUALLY at, which is \
             what the caller re-reads from"
        ),
        other => panic!("the expectation must be answered by the port's conflict, got {other}"),
    }
    // NoStream is the same question asked by a writer that believes it is first.
    assert!(
        matches!(
            store.append("run", ExpectedRevision::NoStream, &h1),
            Err(StoreError::Conflict { .. })
        ),
        "a stream that exists is not a stream that does not, whatever the guard would do \
         with the batch"
    );

    // The CONTROL, and the reason the two above are not simply a store that refuses
    // everything: the current expectation succeeds, writes nothing, and moves nothing.
    let accepted = store
        .append(
            "run",
            ExpectedRevision::Exact((h1.len() - 1) as Revision),
            &h1,
        )
        .expect("the current expectation is honored");
    assert_eq!(accepted.handed(), h1.len(), "one slot per event handed in");
    assert_eq!(
        accepted.written(),
        0,
        "and the guard still suppressed the whole batch - the expectation was settled \
         BEFORE that verdict, not instead of it"
    );

    let after = store.read_stream("run", 0, Direction::Forward).unwrap();
    assert_eq!(
        after.iter().map(|e| e.revision).collect::<Vec<_>>(),
        landed.iter().map(|e| e.revision).collect::<Vec<_>>(),
        "a refused append and an all-suppressed one leave the stream exactly where it was, \
         so the next writer's expectation is still the revision it just read"
    );
}
