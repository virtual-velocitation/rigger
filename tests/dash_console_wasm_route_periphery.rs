//! Periphery (contract / API / integration) test for spec 93 criterion 3, THE BUILD EMBEDS
//! IT: the `/console/core.wasm` route, driven over a REAL socket.
//!
//! `src/dash.rs`'s own inside-out unit test
//! (`console_core_wasm_route_serves_the_embedded_artifact_as_application_wasm`) calls the
//! private `route` function directly, in-process - it never crosses `handle_conn`, the TCP
//! socket, or real HTTP response framing, and it never observes what a REAL client actually
//! receives on the wire. Every OTHER served route in this crate that has a real-socket
//! periphery test (e.g. `tests/dash_calls_route_periphery.rs`) drives `dash::serve_on` over
//! a bound `TcpListener` for exactly that reason - this file closes the SAME gap for the one
//! route that serves BINARY content instead of JSON or HTML, which is a genuinely different
//! wire case every existing real-socket dash test sidesteps: the response body must survive
//! byte-for-byte across a raw socket read (`read_to_string` would panic or corrupt non-UTF-8
//! bytes), and `Content-Length` must name the ACTUAL byte count of a binary payload rather
//! than a JSON string's character count.
//!
//! Two items this file deliberately does NOT re-prove, both already covered elsewhere at
//! this exact granularity:
//!
//! - The byte-for-byte comparison against the embedded `CONSOLE_CORE_WASM` constant: that
//!   constant is deliberately private (see its own doc comment in `src/dash.rs` - a `pub`
//!   accessor with no real production consumer would itself be a dead-code candidate spec
//!   87's audit must disposition), so its one out-of-crate verification need is already
//!   proven from `src/dash.rs`'s own `#[cfg(test)] mod tests`
//!   (`embedded_artifact_matches_a_fresh_independent_nested_build`, opt-in via
//!   `RIGGER_CONSOLE_WASM_EMBED_VERIFY=1`) - the one legitimate place for it. This file
//!   instead proves the REAL-SOCKET dispatch and framing contract: the served bytes are a
//!   genuine WASM module (the magic header), within the spec's 3 MB budget, described by
//!   headers that correctly name a binary payload, and identical across independent
//!   connections (never recomputed per request).
//! - The read-only 405 guard: `route`'s `method != "GET"` check runs BEFORE any path is even
//!   inspected, and `src/dash.rs`'s own real-socket test
//!   (`a_post_over_a_real_socket_is_refused_without_touching_the_store`) already proves that
//!   guard at this exact real-socket granularity for a representative path - the branch
//!   never reads `path` at all, so it holds for `/console/core.wasm` by construction, not by
//!   a second per-route test.
//!
//! `dash` compiles on BOTH the default and `--no-default-features` lanes (nothing here is
//! feature-gated), so these tests run in both. No reference to any external tool or
//! project; hyphens, never em dashes.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use rigger::contextgraph::{CallGraph, Direction, Graph};
use rigger::dash::{self, DashInputs, InstanceView};

/// The WASM magic header every validator and runtime checks first: the four bytes `\0asm`.
const WASM_MAGIC: [u8; 4] = [0x00, 0x61, 0x73, 0x6d];

/// Spec 93 criterion 3's own size budget.
const THREE_MB: usize = 3 * 1024 * 1024;

/// Serve one real request against a fresh `dash::serve_on` and return the raw response
/// bytes exactly as received over the socket - never `read_to_string`, which would panic on
/// (or silently mangle) a non-UTF-8 binary body. Every provider panics if consulted: this
/// route reads no store, graph, calls, or instance-registry input at all, so a provider
/// call would itself be a dispatch-wiring regression.
fn get_raw(target: &str) -> Vec<u8> {
    let provider = |_instance: Option<&str>| -> Result<DashInputs, String> {
        panic!("the console-core wasm route reads no run-scoped store input")
    };
    let graph_provider = |_instance: Option<&str>| -> Graph {
        panic!("the console-core wasm route opens no whole-graph projection")
    };
    let calls_provider =
        |_instance: Option<&str>,
         _seeds: &[String],
         _dir: Direction,
         _depth: i64,
         _floor: &str|
         -> CallGraph { panic!("the console-core wasm route opens no calls projection") };
    let instances_provider = || -> Vec<InstanceView> {
        panic!("the console-core wasm route reads no instance registry")
    };

    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind an ephemeral loopback port");
    let addr = listener.local_addr().expect("learn the bound port");
    // Never joined: `serve_on` loops over `listener.incoming()` for the life of the
    // process, exactly like every other real-socket dash test in this tree drives it.
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

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut client = loop {
        match TcpStream::connect(addr) {
            Ok(s) => break s,
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            Err(e) => panic!("never connected to the served dash on {addr}: {e}"),
        }
    };
    client
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("set a read timeout on the client");
    let req = format!("GET {target} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    client.write_all(req.as_bytes()).expect("write the request");
    let mut raw = Vec::new();
    client
        .read_to_end(&mut raw)
        .expect("read the served response to EOF (Connection: close)");
    raw
}

/// Split a raw HTTP/1.1 response into (header text, body bytes) at the blank-line
/// terminator, WITHOUT ever decoding the body as UTF-8 - it is binary. Headers themselves
/// are always plain ASCII, so only that prefix is decoded.
fn split_response(raw: &[u8]) -> (&str, &[u8]) {
    let sep = b"\r\n\r\n";
    let idx = raw
        .windows(sep.len())
        .position(|w| w == sep)
        .expect("a served HTTP response has a header/body terminator");
    let header = std::str::from_utf8(&raw[..idx]).expect("HTTP headers are ASCII/UTF-8");
    let body = &raw[idx + sep.len()..];
    (header, body)
}

fn header_value<'a>(headers: &'a str, name: &str) -> Option<&'a str> {
    headers.lines().find_map(|line| {
        let (k, v) = line.split_once(':')?;
        k.trim().eq_ignore_ascii_case(name).then(|| v.trim())
    })
}

/// Just the (owned) body bytes of one real request - a thin wrapper so a caller that needs
/// only the body (not the headers) never has to hold `split_response`'s borrow of the raw
/// response alive.
fn get_body(target: &str) -> Vec<u8> {
    let raw = get_raw(target);
    split_response(&raw).1.to_vec()
}

#[test]
fn the_served_console_wasm_route_returns_a_real_wasm_module_with_correct_binary_headers() {
    let raw = get_raw("/console/core.wasm");
    let (headers, body) = split_response(&raw);

    let status_line = headers.lines().next().expect("a status line");
    assert!(
        status_line.starts_with("HTTP/1.1 200"),
        "the console-core wasm route serves 200 over the real socket: {status_line}"
    );
    assert_eq!(
        header_value(headers, "Content-Type"),
        Some("application/wasm"),
        "the served content type must be application/wasm over the real socket: {headers}"
    );
    assert_eq!(
        header_value(headers, "Connection"),
        Some("close"),
        "the response must be framed with Connection: close: {headers}"
    );

    let declared_len: usize = header_value(headers, "Content-Length")
        .expect("a Content-Length header must be present")
        .parse()
        .expect("Content-Length must be a valid integer");
    assert_eq!(
        declared_len,
        body.len(),
        "Content-Length must name the actual BINARY body length over the wire, not a \
         string's character count: declared {declared_len}, actual {}",
        body.len()
    );

    // A real WASM module, proven from the served bytes themselves - not from the embedded
    // constant, which is deliberately unreachable from this crate (see this file's own doc
    // comment).
    assert!(
        body.starts_with(&WASM_MAGIC),
        "the served body must start with the WASM magic header {:x?}: got {:x?}",
        WASM_MAGIC,
        &body[..body.len().min(16)]
    );
    assert!(
        !body.is_empty() && body.len() < THREE_MB,
        "the served console-core wasm module is {} bytes, outside (0, 3 MB) (spec 93 \
         criterion 3), proven over the real socket - not just the embedded constant \
         `src/dash.rs`'s own unit test inspects in-process",
        body.len()
    );
}

#[test]
fn two_independent_real_connections_serve_byte_identical_content() {
    // The route serves a compile-time embedded constant, never anything computed or read
    // per request - two independently bound servers, each answering its own connection,
    // must still serve byte-identical content. A regression that served, say, a freshly
    // re-read file or a nondeterministic buffer would be invisible to a single-request
    // test but would surface here.
    let first = get_body("/console/core.wasm");
    let second = get_body("/console/core.wasm");
    assert_eq!(
        first, second,
        "two independent real requests must serve byte-identical console-core wasm content"
    );
}
