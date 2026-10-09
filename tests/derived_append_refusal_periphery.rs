//! Periphery (contract / API / integration) tests for spec 107 criterion 19, THE STORE REFUSES A
//! DERIVED APPEND. These run OUTSIDE the crates, over the root crate's public surface, and guard
//! the boundaries the backend-agnostic contract case is structurally blind to.
//!
//! The contract case hands one bare `&dyn EventStore` a batch and reads the refusal back through
//! that same handle. What it cannot see, and what this file drives, is the shipped composition
//! and the files under it:
//!
//! 1. THE COMPOSED STORE. Every production writer reaches the adapter through a project namespace
//!    and the folding store, which folds what the store placed into `graph.db`. A refused batch
//!    must come back through both wrappers as the named error, unreworded, and must leave BOTH
//!    files as they stood: no row in `events.db` read through a handle of its own, no `applied`
//!    row and no node in `graph.db`, and no log position spent, so the next kept event lands at
//!    the position the refused batch would have taken.
//! 2. THE CLASS TABLE. The refusal reads `retention::class_of`. Over every type the class lists
//!    name and types no list names, the store refuses exactly the types `class_of` answers
//!    derived, and those are exactly the four derived index types.
//! 3. THE UPGRADE. A log recorded before the ledger still holds derived rows. The store keeps
//!    reading them, refuses a new derived event on their stream under an expectation the stream
//!    meets, and appends a kept event after them at the revision they left the stream on.
//!
//! No operator verb reaches this refusal: both ingest sinks record a ledger entry and fold the
//! batch, and `rigger emit` refuses a derived type at its own allowlist before the store.

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use common::cli::applied_positions;
use common::fixtures::{decision_json, def_json, event_of, live_node_ids, PreLedgerStore};
use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    Fold, Projection, KIND_DECISION, TYPE_CODE_ENTITY_EXTRACTED, TYPE_DECISION_MADE,
    TYPE_EDGE_INFERRED,
};
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Appended, Direction, Error, Event, EventStore, ExpectedRevision, Filter};
use rigger::ingest::{folding_into, DERIVED_INDEX_TYPES};
use rigger::retention::{class_of, Class, EPISODIC_TYPES, PERCEPTION_TYPES};

/// The stream every test of this file appends to.
const STREAM: &str = "run";

/// The type a refused append names and the text it says, or a panic naming what the append
/// answered in its place.
fn refused<T: std::fmt::Debug>(answer: Result<T, Error>) -> (String, String) {
    match answer {
        Err(e @ Error::DerivedAppend { .. }) => {
            let said = e.to_string();
            let Error::DerivedAppend { type_ } = e else {
                unreachable!("matched above")
            };
            (type_, said)
        }
        other => panic!("a batch holding a derived event must be refused by name: {other:?}"),
    }
}

/// What [`refused`] answers for a batch whose first derived event is of `type_`: the type and
/// the whole text the store says of it.
fn naming(type_: &str) -> (String, String) {
    (
        type_.to_string(),
        format!(
            "event store: append refused: {type_} is a derived event, which the tree re-derives - \
             the log keeps the ledger entry of its generation and no event of this batch was \
             written"
        ),
    )
}

/// Every row the log file `db` holds, as `(position, revision, type)` in position order, read
/// through a store handle opened for the read alone.
fn rows_of_the_file(db: &Path) -> Vec<(u64, i64, String)> {
    Store::open(db.to_str().unwrap())
        .unwrap()
        .read_all(0, Direction::Forward, &Filter::default())
        .unwrap()
        .into_iter()
        .map(|e| (e.position, e.revision, e.type_))
        .collect()
}

/// The positions `appended` placed its events at, in batch order.
fn positions(appended: &Appended) -> Vec<u64> {
    appended.placed().map(|(_, at)| at).collect()
}

/// A decision `id` governing nothing.
fn decision(id: &str) -> Event {
    event_of(TYPE_DECISION_MADE, decision_json(id, "x", &[], ""))
}

/// The code entity `alpha` of `src/a.rs`, a derived event.
fn code_entity() -> Event {
    event_of(
        TYPE_CODE_ENTITY_EXTRACTED,
        def_json("src/a.rs", "alpha", 1, true),
    )
}

/// The ids of the decisions `graph` holds.
fn decisions(graph: &Projector) -> Vec<String> {
    live_node_ids(&graph.whole().unwrap(), KIND_DECISION)
}

#[test]
fn a_derived_batch_through_the_composed_store_is_refused_by_name_and_spends_nothing_in_either_file()
{
    let dir = tempfile::tempdir().unwrap();
    let events_db = dir.path().join("events.db");
    let graph_db = dir.path().join("graph.db");
    let backend = Store::open(events_db.to_str().unwrap()).unwrap();
    let graph = Projector::open(graph_db.to_str().unwrap(), "test").unwrap();
    let project = Namespaced::new(&backend, "project");
    let said = std::sync::Mutex::new(Vec::<String>::new());
    let log = |line: &str| said.lock().unwrap().push(line.to_string());
    let folding = folding_into(&project, Some(&graph as &dyn Projection), &log);
    let held = || {
        (
            rows_of_the_file(&events_db),
            applied_positions(&graph_db),
            decisions(&graph),
        )
    };
    let nothing = (Vec::new(), Vec::<u64>::new(), Vec::<String>::new());

    // A knowledge event ahead of a derived one: refused whole, through the folding form and
    // through the port, and neither file gains a row.
    let mixed = [decision("d1"), code_entity()];
    assert_eq!(
        refused(folding.append_and_fold(STREAM, ExpectedRevision::Any, &mixed)),
        naming("CodeEntityExtracted"),
        "the folding form hands the adapter's refusal back as it was said"
    );
    assert_eq!(
        refused(folding.append(STREAM, ExpectedRevision::NoStream, &mixed)),
        naming("CodeEntityExtracted"),
        "the port of the composed store answers the same refusal"
    );
    assert_eq!(
        held(),
        nothing,
        "a refused batch is in neither file: not its knowledge event, not a fold of it"
    );

    // The same knowledge event alone is kept: it lands at the log's first position, which no
    // refused batch spent, on a stream the refusals left unborn, and folds.
    let kept = folding
        .append_and_fold(STREAM, ExpectedRevision::NoStream, &[decision("d1")])
        .expect("a batch holding no derived event is appended");
    assert_eq!(
        (positions(&kept.appended), kept.fold),
        (vec![1], Fold::Folded)
    );
    let one = (
        vec![(1, 0, "DecisionMade".to_string())],
        vec![1],
        vec!["d1".to_string()],
    );
    assert_eq!(held(), one);

    // A derived event after it, under the expectation the stream meets: refused, and the stream
    // and the graph stand as the kept event left them.
    assert_eq!(
        refused(folding.append_and_fold(
            STREAM,
            ExpectedRevision::Exact(0),
            &[
                Event::new(TYPE_EDGE_INFERRED, b"{}".to_vec()),
                decision("d2")
            ],
        )),
        naming("EdgeInferred")
    );
    assert_eq!(held(), one);

    // The next kept event takes the next position and the next revision.
    let next = folding
        .append_and_fold(STREAM, ExpectedRevision::Exact(0), &[decision("d2")])
        .expect("the stream appends at the revision the refusal left it on");
    assert_eq!(
        (positions(&next.appended), next.fold),
        (vec![2], Fold::Folded)
    );
    assert_eq!(
        held(),
        (
            vec![
                (1, 0, "DecisionMade".to_string()),
                (2, 1, "DecisionMade".to_string())
            ],
            vec![1, 2],
            vec!["d1".to_string(), "d2".to_string()],
        )
    );
    assert_eq!(
        *said.lock().unwrap(),
        Vec::<String>::new(),
        "a refusal is an error to its caller, never a lost fold said through the log"
    );
}

#[test]
fn the_store_refuses_exactly_the_types_the_class_table_answers_derived() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("events.db").to_str().unwrap()).unwrap();
    let unlisted = [
        "ReviewVerdict",
        "codeentityextracted",
        "CodeEntityExtracted2",
        "Derived",
    ];
    let types: BTreeSet<&str> = PERCEPTION_TYPES
        .into_iter()
        .chain(EPISODIC_TYPES)
        .chain([TYPE_DECISION_MADE])
        .chain(unlisted)
        .collect();
    assert_eq!(
        types.len(),
        PERCEPTION_TYPES.len() + EPISODIC_TYPES.len() + 1 + unlisted.len(),
        "every type asked is asked once"
    );

    let mut refused_types = BTreeSet::new();
    for type_ in &types {
        // One stream per type, so what a type's stream holds is that type's own answer.
        let batch = [Event::new(*type_, b"{}".to_vec())];
        let answer = store.append(type_, ExpectedRevision::NoStream, &batch);
        let held: Vec<String> = store
            .read_stream(type_, 0, Direction::Forward)
            .unwrap()
            .into_iter()
            .map(|e| e.type_)
            .collect();
        match class_of(type_) {
            Class::Derived => {
                assert_eq!(refused(answer), naming(type_));
                assert_eq!(held, Vec::<String>::new(), "{type_} is not written");
                refused_types.insert(*type_);
            }
            Class::Kept => {
                let appended = answer.unwrap_or_else(|e| panic!("{type_} is kept live: {e}"));
                assert_eq!(appended.written(), 1, "{type_} is written");
                assert_eq!(held, vec![type_.to_string()]);
            }
        }
    }
    assert_eq!(
        refused_types,
        BTreeSet::from([
            "CodeEntityExtracted",
            "DocConceptExtracted",
            "DocLinkExtracted",
            "EdgeInferred",
        ]),
        "the refused types are the four derived index types and no other"
    );
    assert_eq!(refused_types, BTreeSet::from(DERIVED_INDEX_TYPES));
}

#[test]
fn a_log_holding_pre_ledger_derived_rows_reads_them_refuses_a_new_one_and_appends_after_them() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("events.db");
    let store = Store::open(db.to_str().unwrap()).unwrap();
    let project = Namespaced::new(&store, "project");
    // The rows a store recorded before the ledger, in the project's own namespace.
    let before_the_ledger = PreLedgerStore {
        db: &db,
        inner: &store,
    };
    Namespaced::new(&before_the_ledger, "project")
        .append(
            STREAM,
            ExpectedRevision::Any,
            &[
                code_entity(),
                Event::new(TYPE_EDGE_INFERRED, b"{}".to_vec()),
            ],
        )
        .unwrap();
    let held = || -> Vec<(i64, String)> {
        project
            .read_stream(STREAM, 0, Direction::Forward)
            .unwrap()
            .into_iter()
            .map(|e| (e.revision, e.type_))
            .collect()
    };
    let pre_ledger = vec![
        (0, "CodeEntityExtracted".to_string()),
        (1, "EdgeInferred".to_string()),
    ];
    assert_eq!(held(), pre_ledger, "the rows of an older store still read");

    assert_eq!(
        refused(project.append(STREAM, ExpectedRevision::Exact(1), &[code_entity()])),
        naming("CodeEntityExtracted"),
        "a derived row the stream already holds admits no new one"
    );
    assert_eq!(held(), pre_ledger);

    let appended = project
        .append(STREAM, ExpectedRevision::Exact(1), &[decision("d1")])
        .expect("a kept event appends after the pre-ledger rows");
    assert_eq!(positions(&appended), vec![3]);
    assert_eq!(
        held(),
        [pre_ledger, vec![(2, "DecisionMade".to_string())]].concat()
    );
}
