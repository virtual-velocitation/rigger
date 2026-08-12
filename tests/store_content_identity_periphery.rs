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
    Filter, Position, Revision, Subscription,
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
/// reason; nothing on the paths under test reads through it.
struct PortDouble {
    report: Vec<Option<Position>>,
    handed: AtomicUsize,
}

impl PortDouble {
    fn new(report: Vec<Option<Position>>) -> Self {
        PortDouble {
            report,
            handed: AtomicUsize::new(0),
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
        assert_eq!(
            events.len(),
            self.report.len(),
            "the double is built for one exact batch size"
        );
        Ok(Appended::from_placements(self.report.clone()))
    }
    fn read_stream(
        &self,
        _stream: &str,
        _from: Revision,
        _dir: Direction,
    ) -> Result<Vec<Event>, StoreError> {
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
fn path_subject_of(key: &str) -> Option<(&str, &str)> {
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
    Some((&key[..file.len() + 1], hash))
}

/// A content-key shape this project never mints: `<subject>|<generation>`. Used to prove
/// the store parses nothing itself.
fn pipe_subject_of(key: &str) -> Option<(&str, &str)> {
    let (subject, generation) = key.split_once('|')?;
    if subject.is_empty() || generation.is_empty() {
        return None;
    }
    Some((&key[..subject.len() + 1], generation))
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

    let index_exists = || {
        let conn = rusqlite::Connection::open(&path_str).expect("an independent reader opens");
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='index' AND name=?1",
                rusqlite::params!["idx_events_content_key"],
                |r| r.get(0),
            )
            .expect("sqlite_master is readable");
        n == 1
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
/// folds there; on a port that wrote nothing it reports the ABSENCE and folds nowhere.
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
        pos,
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
    let none = rigger::mcpserver::emit_event(&silent, "run", Some(&cap2 as &dyn Projection), &args)
        .expect("a store that wrote nothing is not an emit error");
    assert_eq!(
        none, None,
        "the seam answers with an absence rather than a fabricated position"
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
        first.is_some() && second.is_some(),
        "both progress reports were written and say where"
    );
    assert!(
        first < second,
        "two identical progress lines are two facts, at two positions: {first:?} then {second:?}"
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
        message.contains("SpawnResult"),
        "and names the event whose write was lost, so the seam is identifiable: {message}"
    );
    // OBSERVED, not asserted: this path names the event TYPE and the stream, while its
    // sibling `record_result_if_absent` names the SPAWN ID ("the result of {id}"). Both
    // satisfy the contract that matters here - an explicit failure rather than a
    // fabricated position - so the asymmetry is recorded for the reviewing lenses rather
    // than pinned as a promise nobody made.
}
