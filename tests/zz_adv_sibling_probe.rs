//! ADVERSARY PROBE - does pruning project `alpha` also delete a sibling project whose identity
//! EXTENDS it (`alpha-beta`) in a shared backend?

use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Event, EventStore, ExpectedRevision};
use std::time::{Duration, UNIX_EPOCH};

fn keyed(key: &str, secs: u64) -> Event {
    Event::new(
        rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
        br#"{"file":"src/a.rs","name":"alpha","line":1,"kind":"function","fresh":true}"#.to_vec(),
    )
    .with_meta(rigger::ingest::META_REPLAY_KEY, key)
    .with_valid_from(UNIX_EPOCH + Duration::from_secs(secs))
}

fn count(db: &std::path::Path, prefix: &str) -> i64 {
    let c = rusqlite::Connection::open(db).unwrap();
    c.query_row(
        "SELECT COUNT(*) FROM events WHERE substr(stream,1,length(?1)) = ?1",
        [prefix],
        |r| r.get(0),
    )
    .unwrap()
}

#[test]
fn adv_probe_sibling_namespace_bleed() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("shared.db");
    let backend = Store::open(db.to_str().unwrap()).unwrap();
    for project in ["alpha", "alpha-beta"] {
        let store = Namespaced::new(&backend, project);
        let evs: Vec<Event> = (0..6).map(|r| keyed("gc/src/a.rs@h1#0", 100 + r)).collect();
        store
            .append(rigger::conductor::STREAM, ExpectedRevision::Any, &evs)
            .unwrap();
    }
    let a0 = count(&db, "proj-alpha-");
    let b0 = count(&db, "proj-alpha-beta-");
    let pruned = backend
        .prune_derived_index(
            &Namespaced::prefix_for("alpha"),
            rigger::ingest::META_REPLAY_KEY,
            &rigger::ingest::DERIVED_INDEX_TYPES,
        )
        .unwrap();
    let a1 = count(&db, "proj-alpha-");
    let b1 = count(&db, "proj-alpha-beta-");
    eprintln!("pruning ONLY project `alpha` removed {} rows", pruned.total_removed());
    eprintln!("proj-alpha-*      rows: {a0} -> {a1}   (includes the sibling)");
    eprintln!("proj-alpha-beta-* rows: {b0} -> {b1}   <-- the NEIGHBOUR PROJECT");
    assert_eq!(b0, b1, "SIBLING PROJECT ROWS WERE DELETED");
}
