//! The backend-agnostic contract suite: every EventStore implementation must pass
//! it (append ordering, optimistic-concurrency conflict, catch-up replay-then-live),
//! so the embedded SQLite store is a faithful proxy for the KurrentDB server. Both
//! adapters' tests call `assert_contract`.

use std::time::{Duration, Instant};

use super::{Direction, Error, Event, EventStore, ExpectedRevision, Filter};

/// Run every contract check against a store, panicking on any violation.
pub fn assert_contract(store: &dyn EventStore) {
    append_preserves_order(store);
    optimistic_concurrency_conflicts(store);
    subscription_replays_then_goes_live(store);
}

fn append_preserves_order(store: &dyn EventStore) {
    store
        .append(
            "contract-order",
            ExpectedRevision::Any,
            &[Event::new("A", b"1".to_vec())],
        )
        .unwrap();
    store
        .append(
            "contract-order",
            ExpectedRevision::Any,
            &[Event::new("B", b"2".to_vec())],
        )
        .unwrap();
    let events = store
        .read_stream("contract-order", 0, Direction::Forward)
        .unwrap();
    let types: Vec<&str> = events.iter().map(|e| e.type_.as_str()).collect();
    assert_eq!(types, ["A", "B"], "append must preserve order");
}

fn optimistic_concurrency_conflicts(store: &dyn EventStore) {
    store
        .append(
            "contract-oc",
            ExpectedRevision::NoStream,
            &[Event::new("X", b"x".to_vec())],
        )
        .unwrap();
    let err = store.append(
        "contract-oc",
        ExpectedRevision::NoStream,
        &[Event::new("Y", b"y".to_vec())],
    );
    assert!(
        matches!(err, Err(Error::Conflict { .. })),
        "a wrong expected version must conflict, got {err:?}"
    );
}

fn subscription_replays_then_goes_live(store: &dyn EventStore) {
    store
        .append(
            "contract-sub",
            ExpectedRevision::Any,
            &[Event::new("PRE", b"p".to_vec())],
        )
        .unwrap();
    let sub = store.subscribe_all(0, &Filter::default()).unwrap();
    assert!(
        sub.recv_timeout(Duration::from_secs(5)).is_some(),
        "the subscription must replay existing events"
    );
    store
        .append(
            "contract-sub",
            ExpectedRevision::Any,
            &[Event::new("LIVE", b"l".to_vec())],
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(e) = sub.recv_timeout(Duration::from_secs(1)) {
            if e.type_ == "LIVE" {
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "the subscription must deliver live events"
        );
    }
}
