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
//!
//! THE SNAPSHOT half (the `/api/console/snapshot` bootstrap read) and the STREAM's
//! `liveness`/`heartbeat` frames get the SAME real-socket treatment below: `src/dash.rs`'s
//! own inside-out unit tests prove the snapshot body and the `is_console_event` filter
//! against the pure `route` function / private helpers, in-process, but never open a socket
//! and never observe the `liveness`/`heartbeat` frames at all (no test anywhere did, before
//! this file). Every test in this file runs `#[serial]` under one shared key: the
//! heartbeat/liveness-cadence tests override the stream's env-tunable poll cadence
//! (`RIGGER_DASH_STREAM_POLL_MS`/`_LIVENESS_MS`/`_HEARTBEAT_MS`), which are process-global -
//! two tests racing with different overrides in the SAME test binary would corrupt each
//! other's timing assumptions, so this file trades a little wall-clock time for that
//! guarantee rather than risk a flaky cross-test interaction.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serial_test::serial;

use rigger::contextgraph::{CallGraph, Direction, Graph};
use rigger::dash::{self, DashInputs, InstanceView};
use rigger::eventstore::Event;
use rigger::run::META_RUN_ID;

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
#[serial(dash_console_stream_periphery)]
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
#[serial(dash_console_stream_periphery)]
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
#[serial(dash_console_stream_periphery)]
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
#[serial(dash_console_stream_periphery)]
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
#[serial(dash_console_stream_periphery)]
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

/// A plain `GET <path>` over a real socket, decoded as (status code, `Content-Type`, JSON
/// body) - the generic escape hatch this file uses for a route that answers once and closes
/// (`Connection: close`), unlike the SSE routes `open_stream` drives.
fn get_json(addr: std::net::SocketAddr, path: &str) -> (u16, Option<String>, serde_json::Value) {
    let mut client = TcpStream::connect(addr).expect("connect to the served dash");
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set a read timeout on the client");
    let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    client.write_all(req.as_bytes()).expect("write the request");
    let mut raw = String::new();
    client
        .read_to_string(&mut raw)
        .expect("read the served response to EOF (Connection: close)");
    let mut parts = raw.splitn(2, "\r\n\r\n");
    let headers = parts.next().unwrap_or_default();
    let body = parts.next().unwrap_or_default();
    let status = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or(0);
    let content_type = headers.lines().find_map(|line| {
        let (k, v) = line.split_once(':')?;
        k.trim()
            .eq_ignore_ascii_case("content-type")
            .then(|| v.trim().to_string())
    });
    let json = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    (status, content_type, json)
}

/// THE SNAPSHOT (spec 94, criterion 2): `GET /api/console/snapshot` bootstraps a console tab
/// over a REAL socket - run identity, the type-filtered console-event feed, progress lines,
/// liveness ages, and the definition's stage/gate names. `src/dash.rs`'s own inside-out unit
/// test (`console_snapshot_endpoint_filters_events_carries_progress_liveness_and_definitions`)
/// proves this exact body against the pure `route` function, in-process - it never crosses
/// `handle_conn`, real HTTP response framing, or an actual bound socket a browser tab hits.
/// This test closes that gap for THE SNAPSHOT the same way the tests above close it for THE
/// STREAM, and the same way every other new JSON route in this crate gets its own real-socket
/// periphery test (e.g. `tests/dash_run_tree_spine.rs`'s `/api/state` coverage).
#[test]
#[serial(dash_console_stream_periphery)]
fn console_snapshot_is_served_over_a_real_socket_with_the_filtered_feed_and_definition() {
    let store = FakeStore::default();
    store.push_event(
        ev(
            "RunStarted",
            r#"{"run":"r1","spec":"specs/94-the-console-shell-and-the-live-data-plane.md"}"#,
        )
        // `current_run_id`/`current_run_base` scope by this meta key, exactly like a real
        // RunStarted and like `src/dash.rs`'s own pure-function unit test for this same body.
        .with_meta(META_RUN_ID, "r1"),
    );
    store.push_event(ev(
        "SpawnRequested",
        r#"{"id":"u1/implementer#0","unit":"u1","stage":"implement","prompt":"do it"}"#,
    ));
    store.push_event(ev("GateVerdict", r#"{"gate":"cargo test","pass":true}"#));
    // A graph-extraction type sharing the same stream: must be excluded from `events`.
    store.push_event(ev(
        "CodeEntityExtracted",
        r#"{"id":"src/dash.rs::route","kind":"function"}"#,
    ));
    store.push_progress(ev(
        "AgentProgress",
        r#"{"id":"u1/implementer#0","activity":"reading spec"}"#,
    ));
    store
        .liveness
        .lock()
        .unwrap()
        .insert("u1/implementer#0".to_string(), 12);

    let addr = serve_test_dash(store);
    let (status, content_type, v) = get_json(addr, "/api/console/snapshot");

    assert_eq!(
        status, 200,
        "the snapshot must be served over the real socket"
    );
    assert_eq!(
        content_type.as_deref(),
        Some("application/json"),
        "the snapshot's Content-Type must be application/json over the wire"
    );
    assert_eq!(v["run_id"], "r1");
    assert_eq!(
        v["spec"],
        "specs/94-the-console-shell-and-the-live-data-plane.md"
    );
    let feed = v["events"].as_array().expect("events is an array");
    assert_eq!(
        feed.len(),
        3,
        "the graph-extraction event must be excluded over the real socket: {feed:?}"
    );
    assert!(
        feed.iter().all(|e| e["type"] != "CodeEntityExtracted"),
        "{feed:?}"
    );
    assert_eq!(v["progress"][0]["id"], "u1/implementer#0");
    assert_eq!(v["progress"][0]["activity"], "reading spec");
    assert_eq!(v["liveness"]["u1/implementer#0"], 12);
    assert_eq!(v["definition"]["stages"], serde_json::json!(["implement"]));
    assert_eq!(v["definition"]["gates"], serde_json::json!(["cargo test"]));
}

/// A live spawn's liveness map arrives PROMPTLY as its own `liveness` frame naming its ages -
/// on the very first poll after connect (the stream backdates its own cadence clock so a
/// live agent's staleness is never held back behind a full interval), never folded into an
/// `event` or `progress` frame.
#[test]
#[serial(dash_console_stream_periphery)]
fn a_live_liveness_map_arrives_promptly_as_a_liveness_frame_naming_its_ages() {
    let store = FakeStore::default();
    store
        .liveness
        .lock()
        .unwrap()
        .insert("u1/implementer#0".to_string(), 7);
    let addr = serve_test_dash(store);
    let mut stream = open_stream(addr, 0);

    let frame = read_frame(&mut stream).expect("a liveness frame must arrive");
    assert_eq!(frame.event, "liveness", "{frame:?}");
    let data: serde_json::Value = serde_json::from_str(&frame.data).unwrap();
    assert_eq!(data["ages"]["u1/implementer#0"], 7);
}

/// RAII guard: sets an env var for the life of the guard and restores whatever value (or
/// absence) it had before, even if the test panics. The heartbeat-cadence test below must
/// override the stream's env-tunable cadence to observe a 15-second-default frame inside a
/// test timeout; without this guard a panic mid-test would leak the override into whichever
/// test in this SAME process (env vars are process-global) runs next.
struct EnvGuard {
    key: &'static str,
    prev: Option<String>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let prev = std::env::var(key).ok();
        std::env::set_var(key, value);
        EnvGuard { key, prev }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => std::env::set_var(self.key, v),
            None => std::env::remove_var(self.key),
        }
    }
}

/// The `heartbeat` frame fires on its OWN cadence even with zero events, zero progress, and
/// an EMPTY liveness map - proving both that a heartbeat needs no activity to fire, and (by
/// the first frame being a heartbeat rather than a liveness frame) that an empty liveness map
/// never produces a `liveness` frame. Overrides the poll and heartbeat cadence env vars (see
/// this file's own top doc comment for why every test here runs `#[serial]`) so the
/// 15-second production default never has to elapse inside this test.
#[test]
#[serial(dash_console_stream_periphery)]
fn with_no_activity_the_stream_emits_only_prompt_heartbeats_never_a_liveness_frame() {
    let _poll = EnvGuard::set("RIGGER_DASH_STREAM_POLL_MS", "20");
    let _heartbeat = EnvGuard::set("RIGGER_DASH_STREAM_HEARTBEAT_MS", "30");

    let store = FakeStore::default();
    let addr = serve_test_dash(store);
    let mut stream = open_stream(addr, 0);

    let frame = read_frame(&mut stream).expect("a heartbeat frame must arrive");
    assert_eq!(
        frame.event, "heartbeat",
        "with no events, progress, or live spawns, the first frame must be a heartbeat, \
         never a liveness frame for an empty map: {frame:?}"
    );
    assert_eq!(
        frame.data, "{}",
        "a heartbeat frame carries an empty JSON object"
    );
}
