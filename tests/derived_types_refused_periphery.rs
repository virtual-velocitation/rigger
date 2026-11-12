//! The log refuses every derived type: the four extraction types, the generation ledger entry and
//! the three grouping types. Each is re-derived from the tree, so none is ever appended.

use crate::common;

use common::fixtures::{event_of, naming, refused};
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore, ExpectedRevision, Filter};

#[test]
fn derived_types_are_refused_by_the_log() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("events.db");
    let store = Store::open(db.to_str().unwrap()).unwrap();
    for type_ in [
        "CodeEntityExtracted",
        "EdgeInferred",
        "DocConceptExtracted",
        "DocLinkExtracted",
        "GenerationIngested",
        "CommunityAssigned",
        "ConceptDerived",
        "ConceptRealized",
    ] {
        let answer = store.append(
            "run",
            ExpectedRevision::Any,
            &[event_of(type_, serde_json::json!({}))],
        );
        assert_eq!(refused(answer), naming(type_), "{type_} is refused");
    }
    assert_eq!(
        store
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap()
            .len(),
        0,
        "no refused event is written"
    );
}
