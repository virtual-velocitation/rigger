//! Periphery (contract / API / integration) tests for spec 49 criterion 2: the BATCHED-FOLD
//! CADENCE. A file's whole event batch is appended in ONE store append and folded in ONE graph
//! transaction, so the store's per-transaction cost is paid once per file, not once per event (the
//! measured cold-build throughput was transaction-cadence bound, not parse-bound). These run OUTSIDE
//! the crate, over the library's PUBLIC surface (`rigger::...`), so they guard boundaries the
//! inside-out unit tests are structurally blind to:
//!
//!  - the inside-out unit tests drive the run's ingest SINK (a conductor test with in-crate spies)
//!    and the sqlite `Projector::apply_batch` OVERRIDE (in-crate `super::` paths). Nothing there
//!    drives the shared authority `rigger::ingest::append_and_fold_batch` through its PUBLIC boundary
//!    as an external consumer would, so nothing pins its documented POSITION-STAMPING contract (a
//!    single append lands the batch at consecutive positions ending at the returned last, so event
//!    `i` of an `n`-event batch sits at `last - (n - 1) + i`) nor that its fold is BEST-EFFORT (a
//!    fold error never fails an append that already landed durably);
//!  - the implementer unit-tests only the sqlite Projector's OVERRIDE of `apply_batch`; the trait's
//!    DEFAULT `apply_batch` - the backend-agnostic contract any other `Projection` inherits - is
//!    tested nowhere. A backend with no cheaper batch path must still fold every event through
//!    `apply`, in order, short-circuiting on the first error;
//!  - the criterion-1 periphery suite drives only the PER-EVENT `ingest_project`; the new BATCHED
//!    public entries (`ingest_project_batched` / `_paced`) are driven nowhere, so nothing pins that a
//!    file arrives as ONE whole keyed batch and that flattening the batched walk is byte-identical to
//!    the per-event walk it is a thin view over.
//!
//! `append_and_fold_batch`, the trait method, and the sqlite/eventstore backends are all compiled
//! UNCONDITIONALLY, so the append/fold/default-contract tests are UNGATED and run in BOTH feature
//! lanes. The tests that drive the extraction WALK (`ingest_project_batched`) are `symbols`-gated
//! exactly like the sibling ingest suites; a light-lane test pins the walk's no-op there. Both lanes
//! stay green.

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{Error as CgError, Graph, Projection, REL_GOVERNS, TYPE_DECISION_MADE};
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore, ExpectedRevision};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// A `Projection` that records the positions handed to each `apply_batch` call (grouped per call)
/// and counts any per-event `apply`, so a test can prove `append_and_fold_batch` folds a batch
/// through `apply_batch` at the store-assigned positions and NEVER through a per-event `apply`.
/// `fail` makes `apply_batch` return `Err` so the best-effort contract can be exercised.
#[derive(Default)]
struct CapturingProjection {
    batch_positions: Mutex<Vec<Vec<u64>>>,
    per_event_applies: AtomicUsize,
    fail: bool,
}

impl Projection for CapturingProjection {
    fn apply(&self, _e: &Event) -> Result<(), CgError> {
        self.per_event_applies.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn apply_batch(&self, events: &[Event]) -> Result<(), CgError> {
        self.batch_positions
            .lock()
            .unwrap()
            .push(events.iter().map(|e| e.position).collect());
        if self.fail {
            return Err(CgError("fold failed".into()));
        }
        Ok(())
    }
    fn subgraph(&self, _seed: &[String], _depth: i64) -> Result<Graph, CgError> {
        Ok(Graph::default())
    }
    fn resolve(&self, _mention: &str) -> Result<Option<String>, CgError> {
        Ok(None)
    }
}

/// A `Projection` that records the positions handed to `apply` (in call order) and does NOT override
/// `apply_batch`, so it inherits the trait DEFAULT - the backend-agnostic contract under test.
/// `fail_at` makes `apply` return `Err` for one position, so the default's short-circuit is provable.
#[derive(Default)]
struct RecordingProjection {
    applied: Mutex<Vec<u64>>,
    fail_at: Option<u64>,
}

impl Projection for RecordingProjection {
    fn apply(&self, e: &Event) -> Result<(), CgError> {
        if self.fail_at == Some(e.position) {
            return Err(CgError(format!("apply failed at position {}", e.position)));
        }
        self.applied.lock().unwrap().push(e.position);
        Ok(())
    }
    // apply_batch is deliberately NOT overridden: this projection inherits the trait DEFAULT.
    fn subgraph(&self, _seed: &[String], _depth: i64) -> Result<Graph, CgError> {
        Ok(Graph::default())
    }
    fn resolve(&self, _mention: &str) -> Result<Option<String>, CgError> {
        Ok(None)
    }
}

/// The shared authority `rigger::ingest::append_and_fold_batch` stamps each folded event with the
/// STORE-ASSIGNED position and folds the whole batch through `apply_batch` - never a per-event
/// `apply`. Its documented contract ("a single append lands the batch at consecutive positions
/// ending at the returned last position, so event `i` of an `n`-event batch sits at
/// `last - (n - 1) + i`") is asserted nowhere in-crate: the conductor sink test only COUNTS append
/// and fold calls, and the sqlite unit test PRE-SETS positions before folding. This pins the
/// position math at the crate boundary against a real store's own assignment.
#[test]
fn append_and_fold_batch_stamps_the_store_assigned_positions_and_never_folds_per_event() {
    let store = Store::open(":memory:").unwrap();

    // Advance the stream first, so the batch's base position is NOT the trivial 1 - proving the
    // `base = last + 1 - n` math accounts for a non-empty log, not just a fresh one.
    let prior: Vec<Event> = (0..3)
        .map(|i| Event::new("Prior", format!("p{i}").into_bytes()))
        .collect();
    store.append("main", ExpectedRevision::Any, &prior).unwrap();

    // The batch to append-and-fold: three distinct events with UNSET positions - the function must
    // stamp each one from the store's append result before folding it.
    let batch: Vec<Event> = (0..3)
        .map(|i| Event::new("Batched", format!("b{i}").into_bytes()))
        .collect();

    let cap = CapturingProjection::default();
    let last = rigger::ingest::append_and_fold_batch(
        &store,
        Some(&cap as &dyn Projection),
        "main",
        &batch,
    )
    .unwrap();

    // The store's own assignment is the oracle: read the stream and pick out this batch's events by
    // id, in stream order.
    let batch_ids: std::collections::BTreeSet<String> =
        batch.iter().map(|e| e.id.clone()).collect();
    let store_positions: Vec<u64> = store
        .read_stream("main", 0, Direction::Forward)
        .unwrap()
        .iter()
        .filter(|e| batch_ids.contains(&e.id))
        .map(|e| e.position)
        .collect();

    let n = batch.len() as u64;
    let expected: Vec<u64> = ((last + 1 - n)..=last).collect();
    assert_eq!(
        store_positions, expected,
        "the single append lands the batch at CONSECUTIVE positions ending at the returned last"
    );
    assert_eq!(
        *cap.batch_positions.lock().unwrap(),
        vec![expected.clone()],
        "append_and_fold_batch folds exactly ONE batch, each event stamped with its store-assigned \
         position (event i at last-(n-1)+i)"
    );
    assert!(
        expected[0] > 1,
        "the fixture advanced the log so the batch base is not the trivial position 1; got base {}",
        expected[0]
    );
    assert_eq!(
        cap.per_event_applies.load(Ordering::SeqCst),
        0,
        "the batched fold folds the whole batch in one apply_batch and NEVER one event at a time \
         (apply)"
    );
    assert_eq!(
        last,
        *store_positions.last().unwrap(),
        "the returned position is the batch's last durable position"
    );
}

/// `append_and_fold_batch` folds BEST-EFFORT - a fold error never fails an append that already
/// landed durably in the log - and the empty batch is a total no-op. Neither is pinned in-crate: the
/// sqlite unit test asserts the OVERRIDE rolls back, but nothing asserts the ingest authority
/// SWALLOWS that error and still returns the durable last position, nor the empty-batch/`None`-graph
/// paths.
#[test]
fn append_and_fold_batch_is_best_effort_on_fold_error_and_a_no_op_on_an_empty_batch() {
    let store = Store::open(":memory:").unwrap();

    // Empty batch: appends nothing, folds nothing, returns 0, leaves the stream untouched.
    let cap0 = CapturingProjection::default();
    let n =
        rigger::ingest::append_and_fold_batch(&store, Some(&cap0 as &dyn Projection), "main", &[])
            .unwrap();
    assert_eq!(
        n, 0,
        "an empty batch appends nothing and returns position 0"
    );
    assert!(
        cap0.batch_positions.lock().unwrap().is_empty(),
        "an empty batch folds nothing"
    );
    assert!(
        store
            .read_stream("main", 0, Direction::Forward)
            .unwrap()
            .is_empty(),
        "an empty batch leaves the stream untouched"
    );

    // Fold error: a Projection whose apply_batch ERRORS must NOT fail the append - the batch already
    // landed durably. append_and_fold_batch still returns Ok(last), and the events are readable.
    let failing = CapturingProjection {
        fail: true,
        ..Default::default()
    };
    let batch: Vec<Event> = (0..2)
        .map(|i| Event::new("Batched", format!("b{i}").into_bytes()))
        .collect();
    let last = rigger::ingest::append_and_fold_batch(
        &store,
        Some(&failing as &dyn Projection),
        "main",
        &batch,
    )
    .expect("a fold error must NOT fail the append - the batch already landed durably in the log");
    assert_eq!(
        failing.batch_positions.lock().unwrap().len(),
        1,
        "the fold was attempted (and errored) exactly once"
    );
    let stored = store.read_stream("main", 0, Direction::Forward).unwrap();
    assert_eq!(
        stored.len(),
        2,
        "the batch is durably appended even though its fold errored"
    );
    assert_eq!(
        stored.iter().map(|e| e.position).max().unwrap(),
        last,
        "the returned position is the last durably-appended event, fold error notwithstanding"
    );

    // graph = None: appends only, no fold, no panic.
    let store2 = Store::open(":memory:").unwrap();
    let batch2: Vec<Event> = (0..2)
        .map(|i| Event::new("Batched", format!("c{i}").into_bytes()))
        .collect();
    let last2 = rigger::ingest::append_and_fold_batch(&store2, None, "main", &batch2).unwrap();
    assert_eq!(
        store2
            .read_stream("main", 0, Direction::Forward)
            .unwrap()
            .len(),
        2,
        "graph=None still appends the whole batch"
    );
    assert!(
        last2 >= 2,
        "graph=None returns the last appended position; got {last2}"
    );
}

/// End-to-end integration seam: `rigger::ingest::append_and_fold_batch` folds a whole batch through a
/// REAL sqlite `Projector` (its ONE-transaction `apply_batch` override), reached here through the
/// PUBLIC ingest authority rather than the in-crate `super::` path the unit test uses. Building
/// `DecisionMade` events (whose fold is always compiled, so this runs in both lanes) and reading the
/// result back through the public `subgraph` proves the ingest -> contextgraph seam: one live
/// `GOVERNS` edge per decision, folded from the batch alone.
#[test]
fn append_and_fold_batch_folds_a_whole_batch_through_a_real_projector() {
    let store = Store::open(":memory:").unwrap();
    let projector = Projector::open(":memory:", "test").unwrap();

    let decision = |id: &str, path: &str| -> Event {
        let payload =
            serde_json::json!({ "id": id, "summary": "x", "governs": [path], "supersedes": "" });
        Event::new(TYPE_DECISION_MADE, serde_json::to_vec(&payload).unwrap())
    };
    // Positions UNSET: the authority stamps them from the append, then folds the whole batch.
    let batch = vec![
        decision("d1", "a.rs"),
        decision("d2", "b.rs"),
        decision("d3", "c.rs"),
    ];

    rigger::ingest::append_and_fold_batch(
        &store,
        Some(&projector as &dyn Projection),
        "main",
        &batch,
    )
    .unwrap();

    let g = projector
        .subgraph(
            &["a.rs".to_string(), "b.rs".to_string(), "c.rs".to_string()],
            1,
        )
        .unwrap();
    let mut governs: Vec<(String, String)> = g
        .edges
        .iter()
        .filter(|e| e.rel == REL_GOVERNS)
        .map(|e| (e.from.clone(), e.to.clone()))
        .collect();
    governs.sort();
    assert_eq!(
        governs,
        vec![
            ("d1".to_string(), "a.rs".to_string()),
            ("d2".to_string(), "b.rs".to_string()),
            ("d3".to_string(), "c.rs".to_string()),
        ],
        "append_and_fold_batch folds the whole batch through the real Projector: one live GOVERNS \
         edge per decision, from the batch alone"
    );
}

/// The trait DEFAULT `Projection::apply_batch` - the backend-agnostic contract any `Projection` that
/// does NOT override it inherits - folds each event through `apply`, IN ORDER (the result is exactly
/// what folding each event one at a time produces), and SHORT-CIRCUITS on the first `apply` error.
/// The implementer unit-tests only the sqlite OVERRIDE; this default is tested nowhere in-crate.
#[test]
fn projection_default_apply_batch_folds_each_event_through_apply_in_order_and_short_circuits() {
    let ev = |pos: u64| -> Event {
        let mut e = Event::new("X", Vec::new());
        e.position = pos;
        e
    };
    let batch = vec![ev(1), ev(2), ev(3)];

    // The default apply_batch folds every event through apply, in order.
    let default_proj = RecordingProjection::default();
    default_proj.apply_batch(&batch).unwrap();
    assert_eq!(
        *default_proj.applied.lock().unwrap(),
        vec![1, 2, 3],
        "the default apply_batch folds every event through apply, in order"
    );

    // Reference: folding each event through apply one at a time yields the identical record - so the
    // default's RESULT is exactly what applying each event in order would produce.
    let per_event = RecordingProjection::default();
    for e in &batch {
        per_event.apply(e).unwrap();
    }
    assert_eq!(
        *default_proj.applied.lock().unwrap(),
        *per_event.applied.lock().unwrap(),
        "the default apply_batch == folding each event with apply, in order"
    );

    // The default SHORT-CIRCUITS on an apply error (`self.apply(e)?`): a mid-batch failure surfaces
    // and the events after it never fold.
    let failing = RecordingProjection {
        fail_at: Some(2),
        ..Default::default()
    };
    assert!(
        failing.apply_batch(&batch).is_err(),
        "an apply error must surface from the default apply_batch"
    );
    assert_eq!(
        *failing.applied.lock().unwrap(),
        vec![1],
        "the default apply_batch stops at the first apply error: event 1 folded, event 2 errored, \
         event 3 never folded"
    );
}

/// The new BATCHED public entry `ingest_project_batched` hands the sink each file's WHOLE keyed
/// batch at once, and flattening that walk is byte-identical to the PER-EVENT `ingest_project` it is
/// a thin view over. The criterion-1 periphery suite drives only the per-event entry, so nothing
/// else pins the batched entry: that a file arrives as ONE `on_batch` call carrying all its events,
/// that the keys of a batch share one `<prefix>/<file>@<hash>` and enumerate `#0,#1,...`, that
/// batches arrive in sorted file-path order, and that the batching is width-invariant (parse width
/// changes only criterion 1's parallelism, never the per-file batching).
#[cfg(feature = "symbols")]
#[test]
fn ingest_project_batched_hands_whole_file_batches_and_flattens_to_the_per_event_walk() {
    use std::collections::BTreeSet;

    // Each file carries a def AND a reference to it, so its batch is MULTI-EVENT (a
    // CodeEntityExtracted plus an EdgeInferred): "one whole batch per file" is then observably
    // different from "one event at a time".
    let dir = tempfile::tempdir().unwrap();
    for i in 0..4 {
        std::fs::write(
            dir.path().join(format!("m{i}.rs")),
            format!("pub fn def{i}() {{}}\npub fn use{i}() {{ def{i}(); }}\n"),
        )
        .unwrap();
    }
    let root = dir.path().to_str().unwrap();

    type Triple = (String, String, Vec<u8>);
    let triples = |keyed: &[(String, &Event)]| -> Vec<Triple> {
        keyed
            .iter()
            .map(|(k, ev)| (k.clone(), ev.type_.clone(), ev.data.clone()))
            .collect()
    };

    // Drive the BATCHED public entry: one inner Vec per on_batch call (i.e. per file).
    let mut batches: Vec<Vec<Triple>> = Vec::new();
    let bstats = rigger::ingest::ingest_project_batched(root, |keyed| batches.push(triples(keyed)));

    // Drive the PER-EVENT public entry over the same tree.
    let mut per_event: Vec<Triple> = Vec::new();
    let pstats = rigger::ingest::ingest_project(root, |k, ev| {
        per_event.push((k.to_string(), ev.type_.clone(), ev.data.clone()));
    });

    assert!(
        !batches.is_empty(),
        "the four-file fixture yields code-ingest batches"
    );

    // The batched walk is the SAME walk the per-event view flattens.
    let flattened: Vec<Triple> = batches.iter().flatten().cloned().collect();
    assert_eq!(
        flattened, per_event,
        "ingest_project_batched flattened == ingest_project: same keys, types, and payload bytes, \
         in the same order"
    );

    // One on_batch call PER FILE, and at least one file's batch is multi-event (a def + its
    // reference) - so a batch is a whole file's events, never a disguised one-event call.
    assert_eq!(
        batches.len(),
        bstats.batches_emitted,
        "ingest_project_batched calls on_batch exactly once per file batch"
    );
    assert_eq!(
        bstats.batches_emitted, pstats.batches_emitted,
        "the batched and per-event walks emit the same number of file batches"
    );
    assert!(
        batches.iter().any(|b| b.len() >= 2),
        "a file with a def and a reference to it yields a multi-event batch handed as one unit"
    );

    // Within each batch the keys share ONE `<prefix>/<file>@<hash>` and enumerate `#0,#1,...` in
    // order; across batches the `<prefix>/<file>` arrive sorted (the ordered-emit discipline).
    let mut file_order: Vec<String> = Vec::new();
    for b in &batches {
        let mut stems: BTreeSet<String> = BTreeSet::new();
        for (i, (key, _, _)) in b.iter().enumerate() {
            let (stem, idx) = key
                .rsplit_once('#')
                .unwrap_or_else(|| panic!("key {key:?} must be `<prefix>/<file>@<hash>#<i>`"));
            assert_eq!(
                idx.parse::<usize>().unwrap(),
                i,
                "a batch enumerates its events `#0,#1,...` in order; key {key:?}"
            );
            stems.insert(stem.to_string());
        }
        assert_eq!(
            stems.len(),
            1,
            "every event of a file's batch shares one `<prefix>/<file>@<hash>`; got {stems:?}"
        );
        let stem = stems.into_iter().next().unwrap();
        let file = stem
            .rsplit_once('@')
            .unwrap_or_else(|| panic!("stem {stem:?} must be `<prefix>/<file>@<hash>`"))
            .0
            .to_string();
        file_order.push(file);
    }
    let mut sorted = file_order.clone();
    sorted.sort();
    assert_eq!(
        file_order, sorted,
        "batches arrive in sorted `<prefix>/<file>` order; got {file_order:?}"
    );

    // Batching is width-INVARIANT: the paced entry at width 1 hands the identical batches (parse
    // width changes only the code half's parallelism, criterion 1, never the per-file batching).
    let mut serial_batches: Vec<Vec<Triple>> = Vec::new();
    rigger::ingest::ingest_project_batched_paced(root, 1, |keyed| {
        serial_batches.push(triples(keyed))
    });
    assert_eq!(
        serial_batches, batches,
        "ingest_project_batched_paced at width 1 hands the identical per-file batches - batching is \
         width-invariant"
    );
}

/// The light lane compiles no extraction pass, so its `ingest_project_batched` is a no-op that hands
/// the sink NO batches - the batched analogue of the light-lane `ingest_project`, and what lets a
/// cold `graph build` degrade to an empty graph in the light lane. Pinned directly at the public
/// boundary in that lane.
#[cfg(not(feature = "symbols"))]
#[test]
fn light_lane_ingest_project_batched_hands_no_batches() {
    let dir = tempfile::tempdir().unwrap();
    let mut calls = 0usize;
    rigger::ingest::ingest_project_batched(
        dir.path().to_str().unwrap(),
        |_batch: &[(String, &rigger::eventstore::Event)]| calls += 1,
    );
    assert_eq!(
        calls, 0,
        "the light lane compiles no extraction pass, so ingest_project_batched hands no batches"
    );
}
