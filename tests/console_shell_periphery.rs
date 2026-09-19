//! Periphery (contract / API / integration) test for spec 94 criterion 1, THE PAGE IS
//! THE MOCK'S SHELL: the `/console` page and the `/console/fonts/*` assets, driven over
//! a REAL socket - the same real-wire gap `tests/dash_console_wasm_route_periphery.rs`
//! already closes for the sibling static `/console/core.wasm` route (this file mirrors
//! its harness verbatim; see that file's own doc comment for why an in-process
//! `route(...)` call, which `src/dash.rs`'s own unit tests already use, is not enough on
//! its own for a binary/near-binary payload: `Content-Length` must name the actual wire
//! byte count, and a woff2 body must survive a raw socket read byte-for-byte).
//!
//! `dash` compiles on BOTH the default and `--no-default-features` lanes (nothing here
//! is feature-gated), so these tests run in both. No reference to any external tool or
//! project; hyphens, never em dashes.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use rigger::contextgraph::{CallGraph, Direction, Graph};
use rigger::dash::{self, DashInputs, InstanceView};

/// The woff2 format's own magic header, `wOF2`.
const WOFF2_MAGIC: [u8; 4] = *b"wOF2";

/// Serve one real request against a fresh `dash::serve_on` and return the raw response
/// bytes exactly as received over the socket - never `read_to_string`, which would panic
/// on (or silently mangle) a non-UTF-8 binary body such as a woff2 asset. Every provider
/// panics if consulted: neither `/console` nor `/console/fonts/*` reads a store, graph,
/// calls, or instance-registry input at all, so a provider call would itself be a
/// dispatch-wiring regression.
fn get_raw(target: &str) -> Vec<u8> {
    let provider = |_instance: Option<&str>| -> Result<DashInputs, String> {
        panic!("the console shell and its fonts read no run-scoped store input")
    };
    let graph_provider = |_instance: Option<&str>| -> Graph {
        panic!("the console shell and its fonts open no whole-graph projection")
    };
    let calls_provider = |_instance: Option<&str>,
                          _seeds: &[String],
                          _dir: Direction,
                          _depth: i64,
                          _floor: &str|
     -> CallGraph {
        panic!("the console shell and its fonts open no calls projection")
    };
    let instances_provider = || -> Vec<InstanceView> {
        panic!("the console shell and its fonts read no instance registry")
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
/// terminator, WITHOUT ever decoding the body as UTF-8 - a woff2 body is binary. Headers
/// themselves are always plain ASCII, so only that prefix is decoded.
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

#[test]
fn the_served_console_route_returns_the_shell_page_with_correct_headers_over_a_real_socket() {
    let raw = get_raw("/console");
    let (headers, body) = split_response(&raw);

    let status_line = headers.lines().next().expect("a status line");
    assert!(
        status_line.starts_with("HTTP/1.1 200"),
        "the console route serves 200 over the real socket: {status_line}"
    );
    assert_eq!(
        header_value(headers, "Content-Type"),
        Some("text/html; charset=utf-8"),
        "the served content type must be text/html over the real socket: {headers}"
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
        "Content-Length must name the actual body length over the wire: declared \
         {declared_len}, actual {}",
        body.len()
    );

    let body = std::str::from_utf8(body).expect("the console shell is UTF-8 HTML");
    for region in [
        "id=\"app\"",
        "class=\"tabs\"",
        "class=\"dock\"",
        "class=\"scrub\"",
    ] {
        assert!(
            body.contains(region),
            "missing shell region {region:?} over the real socket"
        );
    }
    assert!(
        !body.contains("http://") && !body.contains("https://"),
        "the served page must reference no URL outside its own origin"
    );
}

#[test]
fn the_served_font_route_returns_a_real_woff2_asset_with_correct_binary_headers() {
    let raw = get_raw("/console/fonts/sora/Sora-400.woff2");
    let (headers, body) = split_response(&raw);

    let status_line = headers.lines().next().expect("a status line");
    assert!(
        status_line.starts_with("HTTP/1.1 200"),
        "the font route serves 200 over the real socket: {status_line}"
    );
    assert_eq!(
        header_value(headers, "Content-Type"),
        Some("font/woff2"),
        "the served content type must be font/woff2 over the real socket: {headers}"
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
    assert!(
        body.starts_with(&WOFF2_MAGIC),
        "the served body must start with the woff2 magic header {:x?}: got {:x?}",
        WOFF2_MAGIC,
        &body[..body.len().min(8)]
    );
}

#[test]
fn the_served_license_route_returns_the_ofl_text_over_a_real_socket() {
    let raw = get_raw("/console/fonts/jetbrains-mono/OFL.txt");
    let (headers, body) = split_response(&raw);
    assert!(headers.lines().next().unwrap().starts_with("HTTP/1.1 200"));
    let body = std::str::from_utf8(body).expect("license text is UTF-8");
    assert!(
        body.contains("SIL OPEN FONT LICENSE"),
        "must carry the OFL text: {body}"
    );
}

#[test]
fn an_unknown_font_asset_is_a_plain_404_over_a_real_socket() {
    let raw = get_raw("/console/fonts/does-not-exist.woff2");
    let (headers, _) = split_response(&raw);
    assert!(
        headers.lines().next().unwrap().starts_with("HTTP/1.1 404"),
        "an unmatched font asset is a plain 404: {headers}"
    );
}
