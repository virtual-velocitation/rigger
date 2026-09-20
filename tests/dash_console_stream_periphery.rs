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
use std::time::{Duration, Instant, SystemTime};

use serial_test::serial;

use rigger::contextgraph::{CallGraph, Direction, Graph};
use rigger::dash::{self, DashInputs, InstanceView};
use rigger::eventstore::Event;
use rigger::run::{META_BASE, META_RUN_ID};

/// One SSE frame as this test reads it off the wire: the `event:` name, the optional `id:`
/// line (only the server's `event` frames carry one - `adj-u94c3-verdict-reject-stream-
/// reconnect-duplicate-events`'s fix, the one signal a real `EventSource` needs to send a
/// meaningful `Last-Event-ID` on its own native reconnect) and the `data:` line's raw text
/// (never re-parsed here beyond what a real `EventSource` client itself would see).
#[derive(Debug)]
struct Frame {
    event: String,
    id: Option<String>,
    data: String,
}

/// Read exactly one SSE frame (`event: <name>\nid: <id>\ndata: <json>\n\n`, the `id:` line
/// optional) off `r`, or `None` on EOF.
fn read_frame(r: &mut impl BufRead) -> Option<Frame> {
    let mut event = None;
    let mut id = None;
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
        } else if let Some(v) = line.strip_prefix("id: ") {
            id = Some(v.to_string());
        } else if let Some(v) = line.strip_prefix("data: ") {
            data = Some(v.to_string());
        }
    }
    Some(Frame {
        event: event.unwrap_or_default(),
        id,
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

/// Bind an ephemeral loopback listener and serve `dash::serve_on` over it with the given
/// events `provider`, wired to the SAME fixed graph/calls/instances providers and server
/// config every stream test in this file needs (none of them exercise the whole-graph,
/// directed-call, or instance-registry routes). The one place this file spells `serve_on`'s
/// other five arguments, so [`serve_test_dash`], [`serve_multi_instance_dash`] and
/// [`serve_test_dash_over_fixed_events`] share this exact boilerplate rather than each
/// re-implementing it - flagged as near-duplicate by this project's own duplication audit
/// before this fix consolidated them. Never joined: `serve_on` loops for the life of the
/// process, like every other real-socket dash test in this tree.
fn serve_dash_with_provider<F>(provider: F) -> std::net::SocketAddr
where
    F: Fn(Option<&str>) -> Result<DashInputs, String> + Send + Sync + 'static,
{
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind an ephemeral loopback port");
    let addr = listener.local_addr().expect("learn the bound port");
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

/// Bind and serve a fresh `dash::serve_on` backed by `store`, returning the bound address.
fn serve_test_dash(store: FakeStore) -> std::net::SocketAddr {
    serve_dash_with_provider(
        move |_instance: Option<&str>| -> Result<DashInputs, String> {
            Ok((
                store.events.lock().unwrap().clone(),
                Graph::default(),
                store.progress.lock().unwrap().clone(),
                store.liveness.lock().unwrap().clone(),
            ))
        },
    )
}

/// Bind and serve `dash::serve_on` whose provider selects between DISTINCT named
/// [`FakeStore`]s by the request's `?instance=` selector (spec 50, criterion 3): an exact
/// name match in `named` wins, anything else (absent, empty, or unrecognized) falls back to
/// `default` - mirroring `handle_conn`'s own `instance.as_deref().filter(|s|
/// !s.is_empty())` fallback shape. Lets a test prove a NEW call site (the stream branch's
/// own `?instance=` extraction, threaded across its `std::thread` hand-off into
/// `serve_console_stream`) actually reaches the selected store, the same way
/// [`serve_test_dash`] proves the single-store case.
fn serve_multi_instance_dash(
    default: FakeStore,
    named: &[(&str, FakeStore)],
) -> std::net::SocketAddr {
    let named: Vec<(String, FakeStore)> = named
        .iter()
        .map(|(name, store)| (name.to_string(), store.clone()))
        .collect();
    serve_dash_with_provider(
        move |instance: Option<&str>| -> Result<DashInputs, String> {
            let store = instance
                .and_then(|want| named.iter().find(|(name, _)| name == want))
                .map(|(_, store)| store)
                .unwrap_or(&default);
            Ok((
                store.events.lock().unwrap().clone(),
                Graph::default(),
                store.progress.lock().unwrap().clone(),
                store.liveness.lock().unwrap().clone(),
            ))
        },
    )
}

/// Open a `GET /api/console/stream?since=<since>` connection and hand back a buffered reader
/// positioned right after the response headers, having asserted the framing headers a real
/// `EventSource` cares about.
fn open_stream(addr: std::net::SocketAddr, since: u64) -> BufReader<TcpStream> {
    open_stream_with_progress_since(addr, since, 0)
}

/// Like [`open_stream`], but also names `progress_since` - the snapshot's own
/// `progress_head` cursor a real client threads through so the stream's progress floor is
/// fixed at connect time from it, never from whatever the stream's own first live poll
/// happens to observe (`adj-u94c2-verdict-reject-progress-floor-race`).
fn open_stream_with_progress_since(
    addr: std::net::SocketAddr,
    since: u64,
    progress_since: u64,
) -> BufReader<TcpStream> {
    open_stream_with_query(
        addr,
        &format!("since={since}&progress_since={progress_since}"),
    )
}

/// Open a `GET /api/console/stream?<query>` connection with an arbitrary raw query string
/// (e.g. to name `instance=` alongside `since=`/`progress_since=`), asserting the same
/// framing headers [`open_stream`] does. The one place this file builds the stream request,
/// so `open_stream`/`open_stream_with_progress_since` and any test naming its own extra
/// param share this exact header handling rather than each re-implementing it.
fn open_stream_with_query(addr: std::net::SocketAddr, query: &str) -> BufReader<TcpStream> {
    open_stream_with_query_and_headers(addr, query, &[])
}

/// Connect to `GET /api/console/stream?<query>` and read back the raw status line and a
/// `BufReader` positioned right after the response headers - the one place this file
/// builds the stream request, so every opener below (200-asserting or not) shares this
/// exact connect/write/status-line handling rather than re-implementing it.
fn send_stream_request(
    addr: std::net::SocketAddr,
    query: &str,
    extra_headers: &[(&str, &str)],
) -> (String, BufReader<TcpStream>) {
    let mut client = TcpStream::connect(addr).expect("connect to the served dash");
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set a read timeout on the client");
    let mut req = format!("GET /api/console/stream?{query} HTTP/1.1\r\nHost: localhost\r\n");
    for (name, value) in extra_headers {
        req.push_str(&format!("{name}: {value}\r\n"));
    }
    req.push_str("\r\n");
    client.write_all(req.as_bytes()).expect("write the request");
    let mut reader = BufReader::new(client);

    let mut status = String::new();
    reader.read_line(&mut status).expect("read the status line");
    (status.trim_end_matches(['\r', '\n']).to_string(), reader)
}

/// Like [`open_stream_with_query`], but also names extra request headers (e.g. a real
/// `EventSource`'s own native `Last-Event-ID` on its automatic reconnect - never a value a
/// test picks in place of the server's own `id:` line). Asserts the framing headers a real
/// `EventSource` cares about: every stream this file opens through here is expected to
/// answer 200; a case that is not (the retained-window guard's `410`) reads the status
/// line directly through [`send_stream_request`]/[`open_stream_status_line`] instead.
fn open_stream_with_query_and_headers(
    addr: std::net::SocketAddr,
    query: &str,
    extra_headers: &[(&str, &str)],
) -> BufReader<TcpStream> {
    let (status, mut reader) = send_stream_request(addr, query, extra_headers);
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

/// `console_event_wire`'s additive `recorded_at` field (spec 94 criterion 3,
/// `d-u94c3-wire-event-recorded-at`) reaches a REAL client over the stream's `event` frame -
/// not just `src/dash.rs`'s own inside-out unit test
/// (`console_event_wire_carries_recorded_at`), which proves the JSON shape against the pure
/// function in isolation and never crosses a socket. `console_event_wire` is the SAME
/// function the snapshot's `events` array serializes with (`src/dash.rs:2845`), so this one
/// real-socket proof for the stream's `event` frame covers both call sites of the one
/// wire-shaping authority. Proves the wall-clock second `console::scrub_track`'s hour ticks
/// need actually survives real HTTP/SSE framing - the one hop the in-process tests cannot see.
#[test]
#[serial(dash_console_stream_periphery)]
fn a_console_event_frame_carries_recorded_at_as_unix_seconds_over_the_real_stream() {
    let store = FakeStore::default();
    let addr = serve_test_dash(store.clone());
    let mut stream = open_stream(addr, 0);

    let mut e = ev("UnitIntegrated", r#"{"id":"u1","commit":"abc"}"#);
    e.recorded_at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    store.push_event(e);

    let frame = read_frame(&mut stream).expect("a frame must arrive");
    assert_eq!(frame.event, "event");
    let data: serde_json::Value = serde_json::from_str(&frame.data).expect("frame data is JSON");
    assert_eq!(
        data["recorded_at"], 1_700_000_000,
        "the stream's event frame must carry the event's real recorded_at as unix seconds, \
         the wall-clock second console::scrub_track's hour ticks need: {data:?}"
    );
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

/// `write_sse` carries an `id:` line matching the event's own position for every `event`
/// frame (`adj-u94c3-verdict-reject-stream-reconnect-duplicate-events`'s fix) - the one
/// signal a real `EventSource` needs to keep `lastEventId` current, so its own native
/// reconnect can send a meaningful `Last-Event-ID` instead of replaying from whatever
/// `since=` the URL was constructed with at page-load.
#[test]
#[serial(dash_console_stream_periphery)]
fn an_event_frame_carries_an_id_line_matching_its_position() {
    let store = FakeStore::default();
    let addr = serve_test_dash(store.clone());
    let mut stream = open_stream(addr, 0);

    store.push_event(ev("UnitStarted", r#"{"id":"u1"}"#));

    let frame = read_frame(&mut stream).expect("a frame must arrive");
    assert_eq!(frame.event, "event");
    assert_eq!(
        frame.id.as_deref(),
        Some("1"),
        "the event frame's id: line must match its own position: {frame:?}"
    );
}

/// Regression for `adv-u94c3-stream-reconnect-duplicates-events` /
/// `adj-u94c3-verdict-reject-stream-reconnect-duplicate-events`: an actual DROPPED and
/// RECONNECTED stream, not just a fresh socket given a manually-supplied `since=`. The
/// first connection sees one event and the `id:` line the test above proves; that
/// connection is then dropped with no clean close - exactly what a laptop sleeping, a wifi
/// blip or a backgrounded tab looks like from the server's side - while the URL's own
/// `since=0` stays whatever it was at page-load (a real `EventSource`'s native retry
/// reissues that SAME URL verbatim; `connectStream` never re-evaluates it). The reconnect
/// carries `Last-Event-ID: 1`, exactly what the browser's own reconnect sends automatically
/// using the `id:` line it kept from the first connection - never a value this test invents
/// in `since=`'s place - and the server must resume PAST it, proving the header wins over
/// the stale query value rather than replaying the already-delivered event a second time.
#[test]
#[serial(dash_console_stream_periphery)]
fn a_reconnect_with_last_event_id_resumes_past_it_even_though_since_in_the_url_is_stale() {
    let store = FakeStore::default();
    store.push_event(ev("UnitStarted", r#"{"id":"u1"}"#));
    let addr = serve_test_dash(store.clone());

    // First connection: `since=0`, this tab's first ever connect.
    let mut first = open_stream(addr, 0);
    let frame = read_frame(&mut first).expect("the pre-existing event must arrive");
    assert_eq!(frame.id.as_deref(), Some("1"), "{frame:?}");
    drop(first); // the dropped connection: gone with no clean close.

    store.push_event(ev("UnitIntegrated", r#"{"id":"u1","commit":"abc"}"#));

    // The reconnect: the SAME stale `since=0` the URL still carries, but with
    // `Last-Event-ID: 1` - what a real browser's own native retry sends.
    let mut second = open_stream_with_query_and_headers(
        addr,
        "since=0&progress_since=0",
        &[("Last-Event-ID", "1")],
    );
    let frame2 = read_frame(&mut second).expect("a frame must arrive after reconnect");
    let data2: serde_json::Value = serde_json::from_str(&frame2.data).unwrap();
    assert_eq!(
        data2["type"], "UnitIntegrated",
        "Last-Event-ID must win over the stale since=0 in the URL - the pre-existing \
         event must never be replayed: {data2:?}"
    );
    assert_eq!(data2["position"], 2);
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

/// Regression for the progress-floor race (`adj-u94c2-verdict-reject-progress-floor-race`):
/// a progress line recorded in the gap between a client's snapshot fetch and that same
/// client's subsequent stream connect must still be delivered, not silently dropped. The
/// client threads `progress_since` from the snapshot's own `progress_head` (its last-seen
/// progress position) into the stream, exactly like `since=` already resumes the event feed
/// with no gap - so the stream's floor is fixed at connect time from that cursor, never
/// derived from whatever its own first live poll happens to observe.
///
/// Both lines are already on the store BEFORE the stream ever opens, so a floor derived from
/// the first poll's own MAX (the old, buggy behavior) would swallow line B too, exactly as
/// it did in production - this reproduces the race deterministically, with no timing
/// dependence, rather than racing a sleep against the poll thread.
#[test]
#[serial(dash_console_stream_periphery)]
fn progress_since_resumes_a_line_recorded_before_the_stream_ever_polled() {
    let store = FakeStore::default();
    // Line A (position 1): what the client's OWN snapshot fetch already returned as
    // `progress_head`.
    store.push_progress(ev(
        "AgentProgress",
        r#"{"id":"u1/implementer#0","activity":"reading spec"}"#,
    ));
    // Line B (position 2): recorded in the gap between that snapshot fetch and the stream
    // connecting - already on the store by the time the stream's first poll runs, exactly
    // the window that swallowed a line under the old first-poll-max floor.
    store.push_progress(ev(
        "AgentProgress",
        r#"{"id":"u1/implementer#0","activity":"writing code"}"#,
    ));
    let addr = serve_test_dash(store.clone());

    // Connects naming line A's own position as `progress_since`, exactly what the
    // snapshot's `progress_head` would have told it - so only line B is a delta.
    let mut stream = open_stream_with_progress_since(addr, 0, 1);

    let frame = read_frame(&mut stream).expect("a progress frame must arrive");
    assert_eq!(frame.event, "progress");
    let data: serde_json::Value = serde_json::from_str(&frame.data).unwrap();
    assert_eq!(
        data["activity"], "writing code",
        "progress_since=1 must still deliver line B even though it was already on the \
         store before the stream's first poll ran: {data:?}"
    );
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

/// THE STREAM's own `?instance=` extraction (`handle_conn`'s pre-route branch,
/// `d-u94c2-stream-threading`) is a NEW call site for the spec-50 multi-instance attach
/// selector: it is threaded across a `std::thread` hand-off into `serve_console_stream`'s
/// own poll loop, never through the generic `provider(instance)` call every other route
/// already shares. Proving the SAME extraction mechanism (`query_param` + `percent_decode`)
/// elsewhere (as `tests/cli.rs` already does for `/api/state`/`/api/graph`) does not prove
/// THIS call site's own wiring - a connection naming one instance must observe only that
/// instance's store, never the default project's.
#[test]
#[serial(dash_console_stream_periphery)]
fn the_stream_threads_its_instance_selector_to_the_matching_stores_events() {
    let default_store = FakeStore::default();
    let inst_a_store = FakeStore::default();
    inst_a_store.push_event(ev("UnitIntegrated", r#"{"id":"only-in-inst-a"}"#));
    let addr = serve_multi_instance_dash(default_store.clone(), &[("inst-a", inst_a_store)]);

    let mut stream = open_stream_with_query(addr, "since=0&progress_since=0&instance=inst-a");
    let frame =
        read_frame(&mut stream).expect("a frame must arrive from the selected instance's store");
    assert_eq!(frame.event, "event");
    let data: serde_json::Value = serde_json::from_str(&frame.data).unwrap();
    assert_eq!(
        data["data"]["id"], "only-in-inst-a",
        "the stream must scope to the ?instance= selector's own store: {data:?}"
    );
    assert!(
        default_store.events.lock().unwrap().is_empty(),
        "the default (un-instanced) store must never observe an event pushed to inst-a's own store"
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
    assert_eq!(
        v["progress_head"], 1,
        "the cursor a client threads into ?progress_since= to resume with no gap"
    );
    assert_eq!(v["liveness"]["u1/implementer#0"], 12);
    assert_eq!(v["definition"]["stages"], serde_json::json!(["implement"]));
    assert_eq!(v["definition"]["gates"], serde_json::json!(["cargo test"]));
}

/// THE SNAPSHOT's `base` field (`console_snapshot_json`'s own doc comment: "the run's
/// PERSISTED base (`run::current_run_base`) wins" - the exact same `effective_base` pattern
/// `build_state` already uses for `/api/state`, line-for-line): a run's own persisted base,
/// stamped in `RunStarted`'s `META_BASE` metadata, wins over whatever default `rigger dash`
/// was started with; an unstamped run falls back to that served default. No test anywhere -
/// not the in-process unit test, not any periphery test above - asserted on `base` before
/// this one.
#[test]
#[serial(dash_console_stream_periphery)]
fn console_snapshot_base_prefers_the_runs_persisted_base_over_the_served_default() {
    // No META_BASE recorded: falls back to the default `serve_test_dash` starts with
    // ("origin/main").
    let unstamped = FakeStore::default();
    unstamped.push_event(ev("RunStarted", r#"{"run":"r1","spec":"s"}"#));
    let addr = serve_test_dash(unstamped);
    let (_, _, v) = get_json(addr, "/api/console/snapshot");
    assert_eq!(
        v["base"], "origin/main",
        "an unstamped run must fall back to the served default base: {v:?}"
    );

    // A persisted base wins over that same served default.
    let stamped = FakeStore::default();
    stamped.push_event(
        ev("RunStarted", r#"{"run":"r1","spec":"s"}"#).with_meta(META_BASE, "origin/release-9.9"),
    );
    let addr2 = serve_test_dash(stamped);
    let (_, _, v2) = get_json(addr2, "/api/console/snapshot");
    assert_eq!(
        v2["base"], "origin/release-9.9",
        "a run's own persisted base must win over the served default: {v2:?}"
    );
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

// --- Spec 94, criterion 3's own round-2 fix: THE STREAM's retained-window guard
// (adj-u94c3-r2-verdict-reject-retained-window-gap-refetch) ---

/// Serve `dash::serve_on` whose provider reads back a FIXED `Vec<Event>` exactly as handed
/// in, positions and all - unlike [`FakeStore::push_event`], which renumbers every event to
/// a contiguous `len() + 1`, and would therefore erase the very GAPS a real
/// `prune_derived_index` deletion leaves behind. The events below come from a REAL
/// `rigger::eventstore::sqlite::Store` (never a mocked/hand-picked gap), read back once
/// after the prune so this provider re-serves precisely what the store actually still
/// holds.
fn serve_test_dash_over_fixed_events(events: Vec<Event>) -> std::net::SocketAddr {
    serve_dash_with_provider(
        move |_instance: Option<&str>| -> Result<DashInputs, String> {
            Ok((events.clone(), Graph::default(), Vec::new(), HashMap::new()))
        },
    )
}

/// Open a `GET /api/console/stream?<query>` connection and hand back its raw status line,
/// via [`send_stream_request`], WITHOUT [`open_stream_with_query`]'s own "must answer 200"
/// assertion - the one place this file reads a stream response that is allowed to be
/// anything other than 200.
fn open_stream_status_line(addr: std::net::SocketAddr, query: &str) -> String {
    send_stream_request(addr, query, &[]).0
}

/// A real store holding three recordings of ONE derived-index replay key (positions 1-3, a
/// duplication `prune_derived_index` sheds down to its latest recording) followed by one
/// real console event (position 4) - then PRUNED for real, exactly the way `rigger reset
/// --derived` prunes production data. `prune_derived_index`'s own `rn DESC` window keeps the
/// group's LATEST position (3) and deletes the two earlier duplicates (1, 2), so the store's
/// post-prune floor - the smallest position ANY row still occupies, of ANY type - moves from
/// 1 to 3. A `since=`/`Last-Event-ID` naming 1 or 2 therefore names a position this store
/// has ACTUALLY pruned, not a mocked one: exactly the scenario
/// `adj-u94c3-r2-verdict-reject-retained-window-gap-refetch` requires a test to exercise.
fn store_with_a_real_retained_window_gap() -> (tempfile::TempDir, Vec<Event>) {
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Direction as StoreDirection, EventStore, ExpectedRevision, Filter};

    let dir = tempfile::tempdir().expect("a scratch dir for the real sqlite store");
    let path = dir.path().join("events.db");
    let path = path.to_str().unwrap();
    let store = Store::open(path).expect("open a real sqlite event store");

    let events = vec![
        Event::new("CodeEntityExtracted", br#"{"id":"src/a.rs::x"}"#.to_vec())
            .with_meta(rigger::ingest::META_REPLAY_KEY, "gc/src/a.rs@h1#0"),
        Event::new("CodeEntityExtracted", br#"{"id":"src/a.rs::x"}"#.to_vec())
            .with_meta(rigger::ingest::META_REPLAY_KEY, "gc/src/a.rs@h1#0"),
        Event::new("CodeEntityExtracted", br#"{"id":"src/a.rs::x"}"#.to_vec())
            .with_meta(rigger::ingest::META_REPLAY_KEY, "gc/src/a.rs@h1#0"),
        Event::new("DecisionMade", br#"{"id":"d1","summary":"kept"}"#.to_vec()),
    ];
    store
        .append("run", ExpectedRevision::Any, &events)
        .expect("seed the real store");

    let pruned = store
        .prune_derived_index("", &rigger::ingest::derived_index_identity())
        .expect("a real prune_derived_index run");
    assert!(
        pruned.total_removed() > 0,
        "the seeded duplicates must actually be deleted, not a no-op: {pruned:?}"
    );

    let remaining = store
        .read_all(0, StoreDirection::Forward, &Filter::default())
        .expect("read back what the store actually still holds, post-prune");
    let floor = remaining
        .iter()
        .map(|e| e.position)
        .min()
        .expect("the store still holds the surviving duplicate and the console event");
    assert_eq!(
        floor, 3,
        "the prune must delete positions 1 and 2 and keep the group's latest (3): {remaining:?}"
    );
    (dir, remaining)
}

/// A reconnecting client naming a `since=`/`Last-Event-ID` the store has actually pruned
/// receives a REAL, distinct non-200 response (never a bare closed socket, which a real
/// `EventSource` would just retry forever at the same stale position) -
/// `adj-u94c3-r2-verdict-reject-retained-window-gap-refetch`'s own required fix. `since=`
/// naming the surviving floor (3) EXACTLY is a different story: not-less-than the floor is
/// not a gap, so that connects normally - proving the guard is a STRICT `since < floor`
/// comparison over the SAME real prune-created gap, never an off-by-one that also refuses
/// the boundary.
#[test]
#[serial(dash_console_stream_periphery)]
fn a_since_strictly_below_the_stores_real_prune_floor_is_refused_but_the_floor_itself_streams() {
    let (_dir, remaining) = store_with_a_real_retained_window_gap();
    let addr = serve_test_dash_over_fixed_events(remaining);

    let refused = open_stream_status_line(addr, "since=2&progress_since=0");
    assert!(
        refused.starts_with("HTTP/1.1 410"),
        "a since= the store has actually pruned must be refused with a distinct non-200, \
         not served as an ordinary 200 stream: {refused}"
    );

    let at_floor = open_stream_status_line(addr, "since=3&progress_since=0");
    assert!(
        at_floor.starts_with("HTTP/1.1 200"),
        "since= exactly at the floor is not beyond it: {at_floor}"
    );
}

/// Recovery: reconnecting from `since=0` (the "no prior cursor" sentinel a page-side
/// snapshot re-fetch + `connectStream(0)`... in practice `connectStream(snapshot.head)`,
/// exercised here at its own `since=0` floor case) after the SAME real prune delivers
/// exactly the current console event, never the deleted duplicates and never a corrupted or
/// missing fold - proving the recovery a `410` is meant to trigger actually lands a client
/// on the CORRECT current state, not merely on SOME response.
#[test]
#[serial(dash_console_stream_periphery)]
fn reconnecting_from_scratch_after_the_real_prune_delivers_exactly_the_surviving_console_event() {
    let (_dir, remaining) = store_with_a_real_retained_window_gap();
    let addr = serve_test_dash_over_fixed_events(remaining);

    let mut stream = open_stream(addr, 0);
    let frame = read_frame(&mut stream).expect("a frame must arrive");
    assert_eq!(
        frame.event, "event",
        "the surviving CodeEntityExtracted row is a graph-extraction type, never an event \
         frame; the first (and only) event frame must be the console event: {frame:?}"
    );
    let data: serde_json::Value = serde_json::from_str(&frame.data).unwrap();
    assert_eq!(data["type"], "DecisionMade");
    assert_eq!(
        data["position"], 4,
        "the fold must land on the console event's real position, not a renumbered one"
    );
}
