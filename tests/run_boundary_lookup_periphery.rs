//! Periphery (contract / API / integration) tests for spec 101 criterion 1: THE BOUNDARY IS A
//! QUERY. `EventStore::last_position(stream, type)` answers the per-stream revision of the newest
//! event of a type on a stream from the backend's own lookup. These run OUTSIDE the crate, over
//! the library's PUBLIC surface (`rigger::...`), and guard the boundaries the inside-out tests are
//! structurally blind to:
//!
//!  - the in-crate tests open `:memory:` stores, so nothing proves an `events.db` FILE written
//!    before the stream-and-type index existed gains that index when the upgraded binary opens it,
//!    nor that the lookup answers the same boundary across a reopen;
//!  - the in-crate namespace test drives the decorator alone; nothing drives the product's own
//!    composition (a project namespace over a file-backed store) as an external caller holds it,
//!    with streams interleaved so a per-stream revision and a global position differ;
//!  - the counting store double is the ONE shared instrument every later criterion asserts a read
//!    cost with, and nothing pins that it records each call exactly - a failed one and a
//!    subscription's deliveries included - and forwards it unchanged.

mod common;

use common::fixtures::{ev, CountedRead, ReadCountingStore};
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore, ExpectedRevision, Filter};
use std::time::Duration;

/// The columns of `index`, in key order, as the database file itself records them.
fn index_columns(conn: &rusqlite::Connection, index: &str) -> Vec<String> {
    // A SELECT (unlike a bare PRAGMA) re-reads a schema another connection changed.
    let mut stmt = conn
        .prepare("SELECT name FROM pragma_index_info(?1) ORDER BY seqno")
        .unwrap();
    let cols = stmt
        .query_map([index], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    cols
}

/// Given an `events.db` written before the stream-and-type index existed, when the store opens it,
/// then the index is created over (stream, type, position) and the boundary lookup answers the
/// newest match of each type on each project's own stream, by per-stream revision.
#[test]
fn an_events_db_from_before_the_boundary_index_gains_it_on_open_and_answers_the_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("events.db");
    let path = db.to_str().unwrap();
    {
        let backend = Store::open(path).unwrap();
        let alpha = Namespaced::new(&backend, "alpha");
        let beta = Namespaced::new(&backend, "beta");
        // Interleaved appends: a project's revisions and the log's global positions diverge.
        alpha
            .append(
                "run",
                ExpectedRevision::NoStream,
                &[ev("RunStarted", "{}"), ev("W", "{}")],
            )
            .unwrap();
        beta.append("run", ExpectedRevision::NoStream, &[ev("W", "{}")])
            .unwrap();
        alpha
            .append(
                "run",
                ExpectedRevision::Exact(1),
                &[ev("RunStarted", "{}"), ev("W", "{}"), ev("W", "{}")],
            )
            .unwrap();
        beta.append("run", ExpectedRevision::Exact(0), &[ev("RunStarted", "{}")])
            .unwrap();
    }
    // The store as an older binary left it: no stream-and-type index.
    let raw = rusqlite::Connection::open(&db).unwrap();
    raw.execute_batch("DROP INDEX idx_events_stream_type;")
        .unwrap();
    assert_eq!(
        index_columns(&raw, "idx_events_stream_type"),
        Vec::<String>::new(),
        "precondition: the file holds no stream-and-type index"
    );

    let backend = Store::open(path).unwrap();
    assert_eq!(
        index_columns(&raw, "idx_events_stream_type"),
        ["stream", "type", "position"],
        "opening an older file creates the lookup's index over (stream, type, position)"
    );
    let alpha = Namespaced::new(&backend, "alpha");
    let beta = Namespaced::new(&backend, "beta");
    // alpha holds revisions 0..=4 at global positions 1, 2, 4, 5, 6.
    assert_eq!(alpha.last_position("run", "RunStarted").unwrap(), Some(2));
    assert_eq!(alpha.last_position("run", "W").unwrap(), Some(4));
    // beta holds revisions 0..=1 at global positions 3 and 7.
    assert_eq!(beta.last_position("run", "RunStarted").unwrap(), Some(1));
    assert_eq!(beta.last_position("run", "W").unwrap(), Some(0));
    assert_eq!(alpha.last_position("run", "Absent").unwrap(), None);
    assert_eq!(
        Namespaced::new(&backend, "gamma")
            .last_position("run", "RunStarted")
            .unwrap(),
        None,
        "a project that never wrote the stream has no boundary"
    );
    assert_eq!(
        backend.last_position("run", "RunStarted").unwrap(),
        None,
        "the unprefixed stream is not any project's stream"
    );

    // The boundary moves with the next run and only with a match of its own type.
    alpha
        .append("run", ExpectedRevision::Exact(4), &[ev("W", "{}")])
        .unwrap();
    assert_eq!(alpha.last_position("run", "RunStarted").unwrap(), Some(2));
    alpha
        .append("run", ExpectedRevision::Exact(5), &[ev("RunStarted", "{}")])
        .unwrap();
    assert_eq!(alpha.last_position("run", "RunStarted").unwrap(), Some(6));
    assert_eq!(beta.last_position("run", "RunStarted").unwrap(), Some(1));

    // A read from the boundary starts AT the boundary event: it is `read_stream`'s inclusive from.
    let from = alpha.last_position("run", "W").unwrap().unwrap();
    let slice = alpha.read_stream("run", from, Direction::Forward).unwrap();
    let types: Vec<(i64, &str)> = slice
        .iter()
        .map(|e| (e.revision, e.type_.as_str()))
        .collect();
    assert_eq!(types, [(5, "W"), (6, "RunStarted")]);
}

/// Given the shared counting double over a real store, when every read, subscribe and lookup
/// method is called through it, then each call is recorded exactly once, in call order, with its
/// start, direction and the events it handed back, while its answer is forwarded unchanged and an
/// append passes through unrecorded.
#[test]
fn the_counting_double_records_every_read_exactly_and_forwards_it_unchanged() {
    let store = Store::open(":memory:").unwrap();
    // "s" holds global positions 1..=4 (revisions 0..=3); "t" holds positions 5 and 6.
    store
        .append(
            "s",
            ExpectedRevision::NoStream,
            &[ev("A", "{}"), ev("B", "{}"), ev("A", "{}"), ev("B", "{}")],
        )
        .unwrap();
    let counted = ReadCountingStore::new(&store);
    counted
        .append(
            "t",
            ExpectedRevision::NoStream,
            &[ev("C", "{}"), ev("D", "{}")],
        )
        .unwrap();
    assert_eq!(
        counted.reads(),
        Vec::<CountedRead>::new(),
        "an append is never a read"
    );
    assert_eq!(counted.materialized(), 0);

    let fwd = counted.read_stream("s", 1, Direction::Forward).unwrap();
    assert_eq!(
        fwd.iter().map(|e| e.revision).collect::<Vec<_>>(),
        [1, 2, 3]
    );
    let back = counted.read_stream("s", 0, Direction::Backward).unwrap();
    assert_eq!(
        back.iter().map(|e| e.revision).collect::<Vec<_>>(),
        [3, 2, 1, 0]
    );
    let t_only = Filter {
        stream_prefix: Some("t".to_string()),
    };
    let all_fwd = counted.read_all(0, Direction::Forward, &t_only).unwrap();
    assert_eq!(
        all_fwd.iter().map(|e| e.position).collect::<Vec<_>>(),
        [5, 6]
    );
    let all_back = counted
        .read_all(1, Direction::Backward, &Filter::default())
        .unwrap();
    assert_eq!(
        all_back.iter().map(|e| e.position).collect::<Vec<_>>(),
        [6, 5, 4, 3, 2]
    );
    let sub = counted.subscribe_stream("s", 2).unwrap();
    let delivered: Vec<(String, i64)> = (0..2)
        .map(|_| sub.recv_timeout(Duration::from_secs(10)).unwrap())
        .map(|e| (e.stream, e.revision))
        .collect();
    assert_eq!(delivered, [("s".to_string(), 2), ("s".to_string(), 3)]);
    drop(sub);
    let sub_all = counted.subscribe_all(5, &Filter::default()).unwrap();
    let first = sub_all.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!((first.stream.as_str(), first.position), ("t", 6));
    drop(sub_all);
    assert_eq!(counted.last_position("s", "A").unwrap(), Some(2));
    assert_eq!(counted.last_position("t", "A").unwrap(), None);

    assert_eq!(
        counted.reads(),
        [
            CountedRead::Stream {
                stream: "s".to_string(),
                from: 1,
                forward: true,
                materialized: 3,
            },
            CountedRead::Stream {
                stream: "s".to_string(),
                from: 0,
                forward: false,
                materialized: 4,
            },
            CountedRead::All {
                from: 0,
                forward: true,
                materialized: 2,
            },
            CountedRead::All {
                from: 1,
                forward: false,
                materialized: 5,
            },
            CountedRead::SubscribeStream {
                stream: "s".to_string(),
                from: 2,
                delivered: 2,
            },
            CountedRead::SubscribeAll {
                from: 5,
                delivered: 1,
            },
            CountedRead::LastPosition {
                stream: "s".to_string(),
                event_type: "A".to_string(),
            },
            CountedRead::LastPosition {
                stream: "t".to_string(),
                event_type: "A".to_string(),
            },
        ]
    );
    assert_eq!(
        counted.materialized(),
        17,
        "the total is the sum of the events every read handed back: 3 + 4 + 2 + 5 + 2 + 1"
    );
    assert_eq!(
        counted
            .reads()
            .iter()
            .map(CountedRead::materialized)
            .collect::<Vec<_>>(),
        [3, 4, 2, 5, 2, 1, 0, 0],
        "a subscription materializes what it delivered; a lookup hands back no event"
    );
}

/// Given the shared counting double over a store whose log is gone, when a stream read, a $all read
/// or a lookup fails, then each failed call is still recorded, in call order, having handed
/// back nothing - a read cost assertion never loses the call that erred.
#[test]
fn the_counting_double_records_a_read_that_fails() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("events.db");
    let store = Store::open(db.to_str().unwrap()).unwrap();
    store
        .append("s", ExpectedRevision::NoStream, &[ev("A", "{}")])
        .unwrap();
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute_batch("DROP TABLE events;")
        .unwrap();
    let counted = ReadCountingStore::new(&store);

    counted
        .read_stream("s", 0, Direction::Forward)
        .expect_err("a read of a dropped log fails");
    counted
        .read_all(0, Direction::Backward, &Filter::default())
        .expect_err("a $all read of a dropped log fails");
    counted
        .last_position("s", "A")
        .expect_err("a lookup in a dropped log fails");

    assert_eq!(
        counted.reads(),
        [
            CountedRead::Stream {
                stream: "s".to_string(),
                from: 0,
                forward: true,
                materialized: 0,
            },
            CountedRead::All {
                from: 0,
                forward: false,
                materialized: 0,
            },
            CountedRead::LastPosition {
                stream: "s".to_string(),
                event_type: "A".to_string(),
            },
        ]
    );
    assert_eq!(counted.materialized(), 0);
}

/// Given the shared counting double stacked on a project namespace over a file-backed store, when
/// a stream subscription replays, goes live, and then the backend's log disappears, then every
/// delivered event reaches the caller in order with the project prefix stripped and is counted
/// into its call, and the backend's terminal failure is relayed through both relays with nothing
/// delivered after it - the one `Subscription::map` relay both decorators observe through.
#[test]
fn a_subscription_through_the_double_and_a_namespace_counts_each_delivery_then_relays_the_failure()
{
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("events.db");
    let backend = Store::open(db.to_str().unwrap()).unwrap();
    let alpha = Namespaced::new(&backend, "alpha");
    alpha
        .append(
            "run",
            ExpectedRevision::NoStream,
            &[ev("RunStarted", "{}"), ev("W", "{}")],
        )
        .unwrap();
    let counted = ReadCountingStore::new(&alpha);

    let sub = counted.subscribe_stream("run", 0).unwrap();
    let next = || {
        let e = sub
            .recv_timeout(Duration::from_secs(10))
            .expect("the relay delivers the event");
        (e.stream, e.revision, e.type_)
    };
    let replayed = [next(), next()];
    assert_eq!(
        replayed,
        [
            ("run".to_string(), 0, "RunStarted".to_string()),
            ("run".to_string(), 1, "W".to_string()),
        ],
        "the replay reaches the caller in order, scoped back to the project's own stream name"
    );
    alpha
        .append("run", ExpectedRevision::Exact(1), &[ev("Live", "{}")])
        .unwrap();
    assert_eq!(next(), ("run".to_string(), 2, "Live".to_string()));

    rusqlite::Connection::open(&db)
        .unwrap()
        .execute_batch("DROP TABLE events;")
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while sub.err().is_none() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        sub.err(),
        Some("no such table: events".to_string()),
        "the backend's terminal failure carries through the namespace and the double"
    );
    assert_eq!(
        sub.recv_timeout(Duration::from_millis(200))
            .map(|e| e.type_),
        None,
        "nothing is delivered after the terminal failure"
    );
    drop(sub);

    assert_eq!(
        counted.reads(),
        [CountedRead::SubscribeStream {
            stream: "run".to_string(),
            from: 0,
            delivered: 3,
        }],
        "the call records the caller's stream name and every event the relay handed on"
    );
    assert_eq!(counted.materialized(), 3);
}
