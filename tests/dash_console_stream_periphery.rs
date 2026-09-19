//! Periphery (contract / API / integration) test for spec 94, criterion 2: THE SNAPSHOT AND
//! THE STREAM's stream half, `GET /api/console/stream?since=N`, driven over a REAL socket.
//!
//! `src/dash.rs`'s own inside-out unit tests prove `is_console_event`/`console_event_wire`
//! and the `/api/console/snapshot` body against the pure `route` function - they never open a
//! socket, so they cannot prove the actual `text/event-stream` wire framing, the `since=`
//! resume, the append-to-delivery latency, or that two connections can be open AT ONCE. This
//! file closes exactly that gap, the same way every other real-socket dash periphery test in
//! this tree does (e.g. `tests/dash_console_wasm_route_periphery.rs`).
//!
//! The stream never subscribes to a live `EventStore`: it re-consults the SAME per-request
//! `provider` closure every other route already reads, on a short poll (`d-u94c2-stream-
//! threading`). These tests exercise that exact contract with a `Mutex`-backed fake store the
//! test itself appends to between reads - proving the SERVED behavior, not a particular
//! backend.
//!
//! `dash` compiles on both the default and `--no-default-features` lanes (nothing here is
//! feature-gated). No reference to any external tool or project; hyphens, never em dashes.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rigger::contextgraph::{CallGraph, Direction, Graph};
use rigger::dash::{self, DashInputs, InstanceView};
use rigger::eventstore::Event;

/// One SSE frame as this test reads it off the wire: the `event:` name and the `data:`
/// line's raw text (never re-parsed here beyond what a real `EventSource` client itself
/// would see - one event name, one data line, per the server's own `write_sse`).
#[derive(Debug)]
struct Frame {
    event: String,
    data: String,
}

/// Read exactly one SSE frame (`event: <name>\ndata: <json>\n\n`) off `r`, or `None` on EOF.
fn read_frame(r: &mut impl BufRead) -> Option<Frame> {
    let mut event = None;
    let mut data = None;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).ok()? == 0 {
            return None; // EOF mid-frame: the connection closed.
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some(v) = line.strip_prefix("event: ") {
            event = Some(v.to_string());
        } else if let Some(v) = line.strip_prefix("data: ") {
            data = Some(v.to_string());
        }
    }
    Some(Frame {
        event: event.unwrap_or_default(),
        data: data.unwrap_or_default(),
    })
}

/// A fake store the test can append to between reads: `events`/`progress`/`liveness` are
/// shared with the `provider` closure `serve_test_dash` builds, so pushing onto them is
/// exactly like a real conductor appending to the real store between two of the stream's
/// own polls.
#[derive(Clone, Default)]
struct FakeStore {
    events: Arc<Mutex<Vec<Event>>>,
    progress: Arc<Mutex<Vec<Event>>>,
    liveness: Arc<Mutex<HashMap<String, u64>>>,
}

impl FakeStore {
    fn push_event(&self, mut e: Event) {
        let mut events = self.events.lock().unwrap();
        e.position = events.len() as u64 + 1;
        events.push(e);
    }

    fn push_progress(&self, mut e: Event) {
        let mut progress = self.progress.lock().unwrap();
        e.position = progress.len() as u64 + 1;
        progress.push(e);
    }
}

fn ev(type_: &str, json: &str) -> Event {
    Event::new(type_, json.as_bytes().to_vec())
}

/// Bind and serve a fresh `dash::serve_on` backed by `store`, returning the bound address.
/// Never joined: `serve_on` loops for the life of the process, like every other real-socket
/// dash test in this tree.
fn serve_test_dash(store: FakeStore) -> std::net::SocketAddr {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind an ephemeral loopback port");
    let addr = listener.local_addr().expect("learn the bound port");
    let provider = move |_instance: Option<&str>| -> Result<DashInputs, String> {
        Ok((
            store.events.lock().unwrap().clone(),
            Graph::default(),
            store.progress.lock().unwrap().clone(),
            store.liveness.lock().unwrap().clone(),
        ))
    };
    let graph_provider = |_instance: Option<&str>| Graph::default();
    let calls_provider =
        |_: Option<&str>, _: &[String], _: Direction, _: i64, _: &str| CallGraph::default();
    let instances_provider = Vec::<InstanceView>::new;
    std::thread::spawn(move || {
        let _ = dash::serve_on(
            listener,
            provider,
            graph_provider,
            calls_provider,
            instances_provider,
            3,
            "rigger-run",
            "origin/main",
        );
    });
    addr
}

/// Open a `GET /api/console/stream?since=<since>` connection and hand back a buffered reader
/// positioned right after the response headers, having asserted the framing headers a real
/// `EventSource` cares about.
fn open_stream(addr: std::net::SocketAddr, since: u64) -> BufReader<TcpStream> {
    let mut client = TcpStream::connect(addr).expect("connect to the served dash");
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set a read timeout on the client");
    let req = format!("GET /api/console/stream?since={since} HTTP/1.1\r\nHost: localhost\r\n\r\n");
    client.write_all(req.as_bytes()).expect("write the request");
    let mut reader = BufReader::new(client);

    let mut status = String::new();
    reader.read_line(&mut status).expect("read the status line");
    assert!(
        status.starts_with("HTTP/1.1 200"),
        "the stream must answer 200: {status}"
    );
    let mut content_type = None;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("read a header line");
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-type") {
                content_type = Some(v.trim().to_string());
            }
        }
    }
    assert_eq!(
        content_type.as_deref(),
        Some("text/event-stream"),
        "the stream's Content-Type must be text/event-stream"
    );
    reader
}

/// THE STREAM's type filter + delivery latency: a console event appended AFTER the
/// connection opens reaches the client as an `event` frame carrying console-core's own wire
/// shape, well within the spec's one-second bound; a graph-extraction type appended right
/// alongside it never does.
#[test]
fn a_newly_appended_console_event_arrives_as_an_event_frame_and_a_graph_type_never_does() {
    let store = FakeStore::default();
    let addr = serve_test_dash(store.clone());
    let mut stream = open_stream(addr, 0);

    let appended_at = Instant::now();
    store.push_event(ev("UnitIntegrated", r#"{"id":"u1","commit":"abc123"}"#));
    // A graph-extraction type sharing the same stream: must NEVER surface as an `event`
    // frame (it would otherwise arrive as frame #2, right behind the console event above).
    store.push_event(ev("CodeEntityExtracted", r#"{"id":"src/dash.rs::route"}"#));
    // A THIRD console event, so if the graph-extraction type were (wrongly) emitted, this
    // frame would be #3, not #2 - proving the filter rather than merely proving SOMETHING
    // arrived.
    store.push_event(ev("UnitEscalated", r#"{"id":"u1"}"#));

    let frame = read_frame(&mut stream).expect("a frame must arrive");
    let elapsed = appended_at.elapsed();
    assert!(
        elapsed < Duration::from_secs(1),
        "an appended console event must reach a connected client within one second \
         (spec 94 c2); took {elapsed:?}"
    );
    assert_eq!(frame.event, "event");
    let data: serde_json::Value = serde_json::from_str(&frame.data).expect("frame data is JSON");
    assert_eq!(data["type"], "UnitIntegrated");
    assert_eq!(data["data"]["commit"], "abc123");
    assert_eq!(data["position"], 1);

    let frame2 = read_frame(&mut stream).expect("a second frame must arrive");
    assert_eq!(frame2.event, "event");
    let data2: serde_json::Value = serde_json::from_str(&frame2.data).expect("frame data is JSON");
    assert_eq!(
        data2["type"], "UnitEscalated",
        "the graph-extraction event (position 2) must be skipped entirely, not just \
         reordered: {data2:?}"
    );
    assert_eq!(data2["position"], 3);
}

/// `since=N` resumes with no gap: a client that connects naming a position already past
/// some events receives only what comes strictly after it, in order - never a replay of
/// what it already has, and never a skip.
#[test]
fn since_resumes_with_no_gap_and_no_replay() {
    let store = FakeStore::default();
    // Two events already on the store BEFORE any client connects.
    store.push_event(ev("UnitStarted", r#"{"id":"u1"}"#));
    store.push_event(ev("UnitStatus", r#"{"id":"u1","status":"green"}"#));
    let addr = serve_test_dash(store.clone());

    // Connects naming the second event's own position: only what comes strictly after it.
    let mut stream = open_stream(addr, 2);
    store.push_event(ev("UnitIntegrated", r#"{"id":"u1","commit":"abc"}"#));

    let frame = read_frame(&mut stream).expect("a frame must arrive");
    let data: serde_json::Value = serde_json::from_str(&frame.data).unwrap();
    assert_eq!(
        data["type"], "UnitIntegrated",
        "since=2 must skip the two pre-existing events, not replay them: {data:?}"
    );
    assert_eq!(data["position"], 3);
}

/// A new progress line appended after connect arrives as its own `progress` frame, carrying
/// the reporting spawn's id and activity - never folded into an `event` frame.
#[test]
fn a_new_progress_line_arrives_as_a_progress_frame() {
    let store = FakeStore::default();
    let addr = serve_test_dash(store.clone());
    let mut stream = open_stream(addr, 0);

    store.push_progress(ev(
        "AgentProgress",
        r#"{"id":"u1/implementer#0","activity":"reading spec"}"#,
    ));

    let frame = read_frame(&mut stream).expect("a progress frame must arrive");
    assert_eq!(frame.event, "progress");
    let data: serde_json::Value = serde_json::from_str(&frame.data).unwrap();
    assert_eq!(data["id"], "u1/implementer#0");
    assert_eq!(data["activity"], "reading spec");
}

/// TWO console tabs (Constraints Walk: "independent cursors, no per-viewer server state")
/// each hold their own live stream open AT THE SAME TIME: opening the second must never wait
/// for the first to close, and both must independently observe a later append - proving the
/// stream runs off the accept loop's own thread (`d-u94c2-stream-threading`), not serially.
#[test]
fn two_concurrent_streams_both_observe_the_same_append_without_blocking_each_other() {
    let store = FakeStore::default();
    let addr = serve_test_dash(store.clone());

    let open_deadline = Instant::now() + Duration::from_secs(2);
    let mut first = open_stream(addr, 0);
    // Opening the SECOND stream must not hang behind the first (a regression to one-
    // connection-at-a-time would block this `connect` until the first stream's thread,
    // which never exits on its own, finally returns control to the accept loop).
    let mut second = open_stream(addr, 0);
    assert!(
        Instant::now() < open_deadline,
        "opening a second console stream must not wait on the first"
    );

    store.push_event(ev("UnitIntegrated", r#"{"id":"u1","commit":"abc"}"#));

    for (name, stream) in [("first", &mut first), ("second", &mut second)] {
        let frame = read_frame(stream)
            .unwrap_or_else(|| panic!("the {name} stream must independently observe the append"));
        assert_eq!(frame.event, "event", "{name} stream");
        let data: serde_json::Value = serde_json::from_str(&frame.data).unwrap();
        assert_eq!(data["type"], "UnitIntegrated", "{name} stream: {data:?}");
    }
}

/// Every non-stream route keeps serving normally while a console stream is held open on
/// another connection - the special-cased thread hand-off must not disturb the ordinary
/// one-request dispatch path.
#[test]
fn an_open_stream_never_blocks_an_ordinary_request_on_another_connection() {
    let store = FakeStore::default();
    let addr = serve_test_dash(store);
    let _held_open = open_stream(addr, 0);

    let mut client = TcpStream::connect(addr).expect("connect for the ordinary request");
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    client
        .write_all(b"GET /api/state HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut resp = String::new();
    client.read_to_string(&mut resp).unwrap();
    assert!(
        resp.starts_with("HTTP/1.1 200"),
        "/api/state must still be served while a console stream is open: {resp}"
    );
}
