//! Periphery (contract / API / integration) test for spec 93 criterion 3, THE BUILD EMBEDS
//! IT: the `/console/core.wasm` route, driven over a REAL socket.
//!
//! `crates/rigger-dash/src/dash.rs`'s own inside-out unit test
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
//!   constant is deliberately private (see its own doc comment in `crates/rigger-dash/src/dash.rs` - a `pub`
//!   accessor with no real production consumer would itself be a dead-code candidate spec
//!   87's audit must disposition), so its one out-of-crate verification need is already
//!   proven from `crates/rigger-dash/src/dash.rs`'s own `#[cfg(test)] mod tests`
//!   (`embedded_artifact_matches_a_fresh_independent_nested_build`, opt-in via
//!   `RIGGER_CONSOLE_WASM_EMBED_VERIFY=1`) - the one legitimate place for it. This file
//!   instead proves the REAL-SOCKET dispatch and framing contract: the served bytes are a
//!   genuine WASM module (the magic header), within the spec's 3 MB budget, described by
//!   headers that correctly name a binary payload, and identical across independent
//!   connections (never recomputed per request).
//! - The read-only 405 guard: `route`'s `method != "GET"` check runs BEFORE any path is even
//!   inspected, and `crates/rigger-dash/src/dash.rs`'s own real-socket test
//!   (`a_post_over_a_real_socket_is_refused_without_touching_the_store`) already proves that
//!   guard at this exact real-socket granularity for a representative path - the branch
//!   never reads `path` at all, so it holds for `/console/core.wasm` by construction, not by
//!   a second per-route test.
//!
//! `dash` compiles on BOTH the default and `--no-default-features` lanes (nothing here is
//! feature-gated), so these tests run in both. No reference to any external tool or
//! project; hyphens, never em dashes.

mod common;

use common::served::get_raw;
use common::served::header_value;
use common::served::split_response;

#[path = "common/served_asset.rs"]
mod served_asset;
use served_asset::served_binary_asset;

/// The WASM magic header every validator and runtime checks first: the four bytes `\0asm`.
const WASM_MAGIC: [u8; 4] = [0x00, 0x61, 0x73, 0x6d];

/// Spec 93 criterion 3's own size budget.
const THREE_MB: usize = 3 * 1024 * 1024;

/// Just the (owned) body bytes of one real request - a thin wrapper so a caller that needs
/// only the body (not the headers) never has to hold `split_response`'s borrow of the raw
/// response alive.
fn get_body(target: &str) -> Vec<u8> {
    let raw = get_raw(target);
    split_response(&raw).1.to_vec()
}

#[test]
fn the_served_console_wasm_route_returns_a_real_wasm_module_with_correct_binary_headers() {
    // A real WASM module, proven from the served bytes themselves - not from the embedded
    // constant, which is deliberately unreachable from this crate (see this file's own doc
    // comment).
    let (headers, body) = served_binary_asset(
        "/console/core.wasm",
        "console-core wasm route",
        "application/wasm",
        "WASM",
        &WASM_MAGIC,
    );
    assert_eq!(
        header_value(&headers, "Connection"),
        Some("close"),
        "the response must be framed with Connection: close: {headers}"
    );
    assert!(
        !body.is_empty() && body.len() < THREE_MB,
        "the served console-core wasm module is {} bytes, outside (0, 3 MB) (spec 93 \
         criterion 3), proven over the real socket - not just the embedded constant \
         `crates/rigger-dash/src/dash.rs`'s own unit test inspects in-process",
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
