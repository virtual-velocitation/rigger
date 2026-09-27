//! The one check of a BINARY asset served by the real dash over a real socket, shared by the
//! suites that serve one (the console's fonts, its wasm core). Included by path
//! (`#[path = "common/served_asset.rs"] mod served_asset;`) next to `mod common;`, whose
//! `served` helpers it drives.

use crate::common::served::{get_raw, header_value, split_response};

/// GETs `target` over a real socket and holds the binary-asset contract: `200` (`route` names
/// what is served), the `content_type`, a `Content-Length` naming the BINARY body's byte length
/// (never a string's character count) and a body opening with the `magic` header named
/// `magic_name`. Returns the response headers and the body.
pub fn served_binary_asset(
    target: &str,
    route: &str,
    content_type: &str,
    magic_name: &str,
    magic: &[u8],
) -> (String, Vec<u8>) {
    let raw = get_raw(target);
    let (headers, body) = split_response(&raw);

    let status_line = headers.lines().next().expect("a status line");
    assert!(
        status_line.starts_with("HTTP/1.1 200"),
        "the {route} serves 200 over the real socket: {status_line}"
    );
    assert_eq!(
        header_value(headers, "Content-Type"),
        Some(content_type),
        "the served content type must be {content_type} over the real socket: {headers}"
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
        body.starts_with(magic),
        "the served body must start with the {magic_name} magic header {magic:x?}: got {:x?}",
        &body[..body.len().min(16)]
    );
    (headers.to_string(), body.to_vec())
}
