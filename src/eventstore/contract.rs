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
    backward_stream_read_reverses_set(store);
    forward_stream_read_honors_nonzero_from(store);
    backward_all_read_reverses_set(store);
    all_position_round_trips_into_read_and_subscribe(store);
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

/// A backward stream read returns the same set as a forward read, reversed -
/// `from` is an inclusive lower bound on revision and direction only flips order.
fn backward_stream_read_reverses_set(store: &dyn EventStore) {
    store
        .append(
            "c-back",
            ExpectedRevision::Any,
            &[
                Event::new("E0", b"0".to_vec()),
                Event::new("E1", b"1".to_vec()),
                Event::new("E2", b"2".to_vec()),
                Event::new("E3", b"3".to_vec()),
            ],
        )
        .unwrap();

    let fwd = store.read_stream("c-back", 0, Direction::Forward).unwrap();
    let fwd_types: Vec<&str> = fwd.iter().map(|e| e.type_.as_str()).collect();
    assert_eq!(
        fwd_types,
        ["E0", "E1", "E2", "E3"],
        "forward read must be in ascending revision order"
    );

    let back = store.read_stream("c-back", 0, Direction::Backward).unwrap();
    let back_types: Vec<&str> = back.iter().map(|e| e.type_.as_str()).collect();
    assert_eq!(
        back_types,
        ["E3", "E2", "E1", "E0"],
        "backward read from 0 must return the whole stream in reverse order"
    );
    let back_revs: Vec<i64> = back.iter().map(|e| e.revision).collect();
    assert_eq!(
        back_revs,
        [3, 2, 1, 0],
        "backward read must carry descending revisions, not discard them"
    );

    // A backward read honors a nonzero, mid-stream `from` as an inclusive lower
    // bound: it returns {revision >= from}, reversed - not the whole stream.
    let back_mid = store.read_stream("c-back", 2, Direction::Backward).unwrap();
    let back_mid_types: Vec<&str> = back_mid.iter().map(|e| e.type_.as_str()).collect();
    assert_eq!(
        back_mid_types,
        ["E3", "E2"],
        "backward read from a mid-stream revision must honor `from`, not read from the end"
    );
}

/// A forward stream read from a nonzero `from` includes the boundary event:
/// `from` is an *inclusive* lower bound on revision.
fn forward_stream_read_honors_nonzero_from(store: &dyn EventStore) {
    store
        .append(
            "c-from",
            ExpectedRevision::Any,
            &[
                Event::new("F0", b"0".to_vec()),
                Event::new("F1", b"1".to_vec()),
                Event::new("F2", b"2".to_vec()),
            ],
        )
        .unwrap();

    let from_mid = store.read_stream("c-from", 1, Direction::Forward).unwrap();
    let types: Vec<&str> = from_mid.iter().map(|e| e.type_.as_str()).collect();
    assert_eq!(
        types,
        ["F1", "F2"],
        "a forward read from revision 1 must include revision 1 (inclusive) and what follows"
    );
    assert_eq!(
        from_mid.first().map(|e| e.revision),
        Some(1),
        "the boundary event (revision == from) must be present"
    );
}

/// A backward `$all` read returns the same filtered set as a forward read,
/// reversed.
fn backward_all_read_reverses_set(store: &dyn EventStore) {
    let filter = Filter {
        stream_prefix: Some("c-aback-".to_string()),
    };
    for (i, ty) in ["G0", "G1", "G2"].iter().enumerate() {
        store
            .append(
                &format!("c-aback-{i}"),
                ExpectedRevision::Any,
                &[Event::new(*ty, vec![i as u8])],
            )
            .unwrap();
    }

    let fwd = store.read_all(0, Direction::Forward, &filter).unwrap();
    let fwd_types: Vec<&str> = fwd.iter().map(|e| e.type_.as_str()).collect();
    assert_eq!(
        fwd_types,
        ["G0", "G1", "G2"],
        "forward $all read must be in ascending position order"
    );

    let back = store.read_all(0, Direction::Backward, &filter).unwrap();
    let back_types: Vec<&str> = back.iter().map(|e| e.type_.as_str()).collect();
    assert_eq!(
        back_types,
        ["G2", "G1", "G0"],
        "backward $all read must return the same set as forward, reversed"
    );
}

/// A `$all` position returned from a read round-trips: feeding it back into
/// `read_all` and `subscribe_all` (both **exclusive** on `from`) yields exactly
/// the events after it, identically across read and subscription.
fn all_position_round_trips_into_read_and_subscribe(store: &dyn EventStore) {
    let filter = Filter {
        stream_prefix: Some("c-rt-".to_string()),
    };
    // Four events across four streams so they share the global $all order.
    for (i, ty) in ["P0", "P1", "P2", "P3"].iter().enumerate() {
        store
            .append(
                &format!("c-rt-{i}"),
                ExpectedRevision::Any,
                &[Event::new(*ty, vec![i as u8])],
            )
            .unwrap();
    }

    let all = store.read_all(0, Direction::Forward, &filter).unwrap();
    let all_types: Vec<&str> = all.iter().map(|e| e.type_.as_str()).collect();
    assert_eq!(
        all_types,
        ["P0", "P1", "P2", "P3"],
        "the four round-trip events must read back in order"
    );

    // Take the position of the second event (P1) as a resume checkpoint. The
    // value is opaque and backend-assigned; we only ever feed it back to the
    // same store, so SQLite's 1-based positions and KurrentDB's commit
    // positions both work.
    let checkpoint = all[1].position;

    // read_all from the checkpoint is exclusive: it must yield exactly the
    // events after P1, i.e. P2 and P3 (never re-deliver P1, never drop P2).
    let resumed = store
        .read_all(checkpoint, Direction::Forward, &filter)
        .unwrap();
    let resumed_types: Vec<&str> = resumed.iter().map(|e| e.type_.as_str()).collect();
    assert_eq!(
        resumed_types,
        ["P2", "P3"],
        "read_all from a returned position is exclusive: it must resume strictly after that event"
    );

    // subscribe_all from the same checkpoint must replay the identical set, so a
    // read and a catch-up subscription from one position never diverge at the
    // boundary.
    let sub = store.subscribe_all(checkpoint, &filter).unwrap();
    let replayed = collect_replay(&sub, 2);
    assert_eq!(
        replayed,
        ["P2", "P3"],
        "subscribe_all from a returned position must replay the same set as read_all (exclusive boundary)"
    );

    // The resumed subscription is live: a new matching event arrives.
    store
        .append(
            "c-rt-4",
            ExpectedRevision::Any,
            &[Event::new("P4", b"4".to_vec())],
        )
        .unwrap();
    drain_until(
        &sub,
        "P4",
        "a position-resumed subscription must still go live",
    );
}

/// Collect the next `n` replayed event types from a subscription, failing if
/// they do not arrive in time.
fn collect_replay(sub: &super::Subscription, n: usize) -> Vec<String> {
    let mut got = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while got.len() < n {
        if let Some(e) = sub.recv_timeout(Duration::from_secs(1)) {
            got.push(e.type_);
        }
        assert!(
            Instant::now() < deadline,
            "subscription did not replay {n} events in time (got {got:?})"
        );
    }
    got
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
