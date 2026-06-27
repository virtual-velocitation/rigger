//! The backend-agnostic contract suite: every EventStore implementation must pass
//! it, so the embedded SQLite store is a faithful proxy for the KurrentDB server.
//! Both adapters' tests call `assert_contract`.

use std::time::{Duration, Instant, UNIX_EPOCH};

use super::{Direction, Error, Event, EventStore, ExpectedRevision, Filter};

/// Run every contract check against a store, panicking on any violation.
pub fn assert_contract(store: &dyn EventStore) {
    append_assigns_revisions(store);
    optimistic_concurrency_reports_actual(store);
    meta_and_valid_from_round_trip(store);
    subscription_replays_then_goes_live(store);
    stream_subscription_replays_then_goes_live(store);
}

fn append_assigns_revisions(store: &dyn EventStore) {
    store
        .append(
            "c-rev",
            ExpectedRevision::Any,
            &[
                Event::new("A", b"1".to_vec()),
                Event::new("B", b"2".to_vec()),
            ],
        )
        .unwrap();
    let events = store.read_stream("c-rev", 0, Direction::Forward).unwrap();
    let types: Vec<&str> = events.iter().map(|e| e.type_.as_str()).collect();
    assert_eq!(types, ["A", "B"], "append must preserve order");
    let revs: Vec<i64> = events.iter().map(|e| e.revision).collect();
    assert_eq!(revs, [0, 1], "append must assign per-stream revisions 0,1");
    assert!(
        events.iter().all(|e| e.stream == "c-rev"),
        "the store must stamp the stream on each returned event"
    );
}

fn optimistic_concurrency_reports_actual(store: &dyn EventStore) {
    store
        .append(
            "c-oc",
            ExpectedRevision::NoStream,
            &[Event::new("X", b"x".to_vec())],
        )
        .unwrap();
    let err = store.append(
        "c-oc",
        ExpectedRevision::NoStream,
        &[Event::new("Y", b"y".to_vec())],
    );
    match err {
        Err(Error::Conflict { actual, .. }) => {
            assert_eq!(
                actual, 0,
                "one event written => the stream's actual last revision is 0"
            )
        }
        other => panic!("expected a conflict carrying the actual revision, got {other:?}"),
    }
}

fn meta_and_valid_from_round_trip(store: &dyn EventStore) {
    let vf = UNIX_EPOCH + Duration::from_secs(1_000_000); // a time in the past
    let event = Event::new("M", b"m".to_vec())
        .with_meta("actor", "agent-7")
        .with_valid_from(vf);
    store
        .append("c-meta", ExpectedRevision::Any, &[event])
        .unwrap();
    let got = store.read_stream("c-meta", 0, Direction::Forward).unwrap();
    let got = &got[0];
    assert_eq!(
        got.meta.get("actor").map(String::as_str),
        Some("agent-7"),
        "meta (actor) must round-trip"
    );
    let secs = got
        .valid_from
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    assert_eq!(
        secs, 1_000_000,
        "caller-supplied valid_from must round-trip"
    );
    assert!(
        got.recorded_at > vf,
        "recorded_at must be store-stamped at ingest, not the caller's valid_from"
    );
}

fn subscription_replays_then_goes_live(store: &dyn EventStore) {
    store
        .append(
            "c-sub",
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
            "c-sub",
            ExpectedRevision::Any,
            &[Event::new("LIVE", b"l".to_vec())],
        )
        .unwrap();
    drain_until(&sub, "LIVE", "subscribe_all must deliver live events");
}

fn stream_subscription_replays_then_goes_live(store: &dyn EventStore) {
    store
        .append(
            "c-sub-s",
            ExpectedRevision::Any,
            &[Event::new("PRE", b"p".to_vec())],
        )
        .unwrap();
    let sub = store.subscribe_stream("c-sub-s", 0).unwrap();
    assert!(
        sub.recv_timeout(Duration::from_secs(5)).is_some(),
        "the stream subscription must replay existing events"
    );
    store
        .append(
            "c-sub-s",
            ExpectedRevision::Any,
            &[Event::new("LIVE", b"l".to_vec())],
        )
        .unwrap();
    drain_until(&sub, "LIVE", "subscribe_stream must deliver live events");
}

fn drain_until(sub: &super::Subscription, want_type: &str, msg: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(e) = sub.recv_timeout(Duration::from_secs(1)) {
            if e.type_ == want_type {
                return;
            }
        }
        assert!(Instant::now() < deadline, "{msg}");
    }
}
