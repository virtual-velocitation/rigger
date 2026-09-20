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

/// Spec 94 criterion 1, THE PAGE IS THE MOCK'S SHELL, proven over the REAL SOCKET (not
/// just the in-process `route(...)` call `src/dash.rs`'s own unit test already makes):
/// every one of the seven shell regions the Design names - header, tab bar, health
/// strip, view, dock, scrubber, statusline - is present with the mock's own class
/// names, and the light and dark theme token blocks are BOTH served verbatim (bare
/// `:root`, the system media query, and the explicit `[data-theme="dark"]` override),
/// so an explicit choice always wins over the system default in both directions. A
/// build-time asset bug (a stale embed, a mangled include_str!) would pass the
/// in-process test yet still be caught here, because this reads the bytes the built
/// binary actually serves.
#[test]
fn the_served_console_route_carries_every_named_shell_region_and_both_theme_blocks_over_a_real_socket(
) {
    let raw = get_raw("/console");
    let (_, body) = split_response(&raw);
    let body = std::str::from_utf8(body).expect("the console shell is UTF-8 HTML");

    for region in [
        "id=\"app\"",
        "header class=\"top\"",
        "nav class=\"tabs\"",
        "id=\"health\"",
        "class=\"view\"",
        "aside class=\"dock\"",
        "footer class=\"scrub\"",
        "id=\"statusline\"",
        "id=\"projsel\"",
        "id=\"themebtn\"",
        "id=\"palbtn\"",
    ] {
        assert!(
            body.contains(region),
            "missing shell region {region:?} over the real socket: {body}"
        );
    }
    for (view, label) in [
        ("fleet", "Fleet"),
        ("theater", "Theater"),
        ("agents", "Agents"),
        ("court", "Courtroom"),
        ("map", "Knowledge"),
        ("plan", "Plan"),
        ("brief", "Briefing"),
    ] {
        assert!(
            body.contains(&format!("data-view=\"{view}\"")),
            "missing the {view:?} tab/view region over the real socket: {body}"
        );
        assert!(
            body.contains(label),
            "missing the {label:?} tab label over the real socket: {body}"
        );
    }

    assert!(
        body.contains(":root{")
            && body.contains("--bg:#F2F5F7")
            && body.contains("--accent:#B86F2E"),
        "light-theme tokens must be verbatim on bare :root over the real socket: {body}"
    );
    assert!(
        body.contains("prefers-color-scheme: dark")
            && body.contains(":root:not([data-theme=\"light\"])")
            && body.contains("--bg:#0C141B"),
        "dark tokens must apply under the system media query over the real socket: {body}"
    );
    assert!(
        body.contains(":root[data-theme=\"dark\"]") && body.matches("--bg:#0C141B").count() >= 2,
        "dark tokens must ALSO apply verbatim under an explicit [data-theme=dark] over the \
         real socket: {body}"
    );
}

/// The Design states plainly: "Each view region is empty in this spec except for the
/// sentence naming the spec that fills it." Proven over the real socket for every one
/// of the seven views AND the dock, each pinned to ITS OWN region (not just "the word
/// appears somewhere on the page") - so a later spec that fills the wrong view, or
/// drops the naming while filling its own, is caught here rather than by a person
/// reading the page.
#[test]
fn the_served_console_route_names_the_filling_spec_in_every_empty_region_pinned_to_its_own_region()
{
    let raw = get_raw("/console");
    let (_, body) = split_response(&raw);
    let body = std::str::from_utf8(body).expect("the console shell is UTF-8 HTML");

    for (region, spec) in [
        ("fleet", "97"),
        ("theater", "95"),
        ("agents", "95"),
        ("court", "96"),
        ("map", "98"),
        ("plan", "96"),
        ("brief", "97"),
    ] {
        // Pinned to the `<section class="view" ...>` tag specifically - `data-view="X"`
        // alone would match the nav tab BUTTON of the same name first.
        let marker = format!("class=\"view\" data-view=\"{region}\"");
        let start = body
            .find(&marker)
            .unwrap_or_else(|| panic!("missing the {region:?} view section: {body}"));
        let end = body[start..]
            .find("</section>")
            .map(|i| start + i)
            .unwrap_or(body.len());
        let window = &body[start..end];
        assert!(
            window.contains(&format!("spec {spec}")),
            "the {region:?} view must name spec {spec} as the one that fills it: {window:?}"
        );
    }

    let dock_start = body.find("id=\"dock\"").expect("missing the dock region");
    let dock_end = body[dock_start..]
        .find("</aside>")
        .map(|i| dock_start + i)
        .unwrap_or(body.len());
    let dock_window = &body[dock_start..dock_end];
    assert!(
        dock_window.contains("spec 95"),
        "the dock must name the spec that fills it: {dock_window:?}"
    );
}

/// Spec 94 criterion 1 OWNS "the theme toggle's persistence in the browser's storage",
/// proven the same way this codebase already proves served-page JS contracts, but over
/// the REAL SOCKET this time: the served page's own script both reads the stored theme
/// back on load and writes it on toggle under the same key, so a choice actually
/// round-trips across a reload of the SAME bytes the browser would receive - not just
/// the bytes an in-process function call happens to return.
#[test]
fn the_served_console_route_wires_the_theme_toggles_persistence_over_a_real_socket() {
    let raw = get_raw("/console");
    let (_, body) = split_response(&raw);
    let body = std::str::from_utf8(body).expect("the console shell is UTF-8 HTML");

    assert!(
        body.contains("localStorage.getItem(THEME_KEY)"),
        "must restore the persisted theme on load over the real socket: {body}"
    );
    assert!(
        body.contains("localStorage.setItem(THEME_KEY, next)"),
        "must persist the theme on toggle over the real socket: {body}"
    );
    assert!(
        body.contains("id=\"themebtn\""),
        "the toggle button itself must be present over the real socket: {body}"
    );
}
