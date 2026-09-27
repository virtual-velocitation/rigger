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

mod common;

use common::served::get_raw;
use common::served::header_value;
use common::served::split_response;

#[path = "common/served_asset.rs"]
mod served_asset;
use served_asset::served_binary_asset;

/// The woff2 format's own magic header, `wOF2`.
const WOFF2_MAGIC: [u8; 4] = *b"wOF2";

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
    served_binary_asset(
        "/console/fonts/sora/Sora-400.woff2",
        "font route",
        "font/woff2",
        "woff2",
        &WOFF2_MAGIC,
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
