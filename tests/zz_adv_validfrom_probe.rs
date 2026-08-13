//! ADVERSARY PROBE - not part of the unit. Does a compacted log fold to the SAME live graph,
//! including the bitemporal column the unit's own test excluded (`edges.valid_from`)?

use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore, ExpectedRevision};
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

const KEY_A_DEF: &str = "gc/src/a.rs@h1#0";
const KEY_A_REF: &str = "gc/src/a.rs@h1#1";
const ROUNDS: u64 = 20;

fn code_entity(file: &str, name: &str, line: u32) -> Vec<u8> {
    format!(
        r#"{{"file":"{file}","name":"{name}","line":{line},"kind":"function","fresh":true}}"#
    )
    .into_bytes()
}

fn doc_link() -> Vec<u8> {
    br#"{"from":"docs/ra.md","to":"src/a.rs","rel":"SPECIFIES"}"#.to_vec()
}

fn keyed(type_: &str, data: Vec<u8>, key: &str, secs: u64) -> Event {
    Event::new(type_, data)
        .with_meta(rigger::ingest::META_REPLAY_KEY, key)
        .with_valid_from(UNIX_EPOCH + Duration::from_secs(secs))
}

fn fold_edges(events: &[Event], project: &str, path: &Path) -> Vec<String> {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::Projection;
    {
        let p = Projector::open(path.to_str().unwrap(), project).unwrap();
        p.apply_batch(events).unwrap();
    }
    let conn = rusqlite::Connection::open(path).unwrap();
    let mut edges: Vec<String> = conn
        .prepare("SELECT from_id, to_id, rel, valid_from, source FROM edges WHERE valid_to IS NULL")
        .unwrap()
        .query_map([], |r| {
            Ok(format!(
                "{}|{}|{}|valid_from={}|source={}",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?
            ))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    edges.sort();
    edges
}

#[test]
fn adv_probe_compaction_shifts_valid_from() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("events.db");
    let project = "advprobe";
    let backend = Store::open(db.to_str().unwrap()).unwrap();
    {
        let store = Namespaced::new(&backend, project);
        let mut events = Vec::new();
        for r in 0..ROUNDS {
            events.push(keyed(
                rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                code_entity("src/a.rs", "alpha", 1),
                KEY_A_DEF,
                1_000 + r,
            ));
            events.push(keyed(
                rigger::contextgraph::TYPE_DOC_LINK_EXTRACTED,
                doc_link(),
                KEY_A_REF,
                1_000 + r,
            ));
        }
        store
            .append(rigger::conductor::STREAM, ExpectedRevision::Any, &events)
            .unwrap();
    }

    let read = || -> Vec<Event> {
        let store = Namespaced::new(&backend, project);
        store
            .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
            .unwrap()
    };

    let before = fold_edges(&read(), project, &dir.path().join("before.db"));

    let pruned = backend
        .prune_derived_index(
            &Namespaced::prefix_for(project),
            rigger::ingest::META_REPLAY_KEY,
            &rigger::ingest::DERIVED_INDEX_TYPES,
        )
        .unwrap();

    let after = fold_edges(&read(), project, &dir.path().join("after.db"));

    eprintln!("pruned {} rows", pruned.total_removed());
    eprintln!("BEFORE (fold of the full log):");
    for e in &before {
        eprintln!("  {e}");
    }
    eprintln!("AFTER  (fold of the compacted log):");
    for e in &after {
        eprintln!("  {e}");
    }
    assert_eq!(before, after, "VALID_FROM DIVERGES");
}
