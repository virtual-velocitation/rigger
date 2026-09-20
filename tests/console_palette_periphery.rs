//! Periphery (contract / API / integration) test for spec 94 criterion 4, THE PALETTE -
//! proven at the level Design's own Notes name for a page-side criterion in this spec:
//! "the served page's source is asserted to call those ops and to carry the mock's markup
//! and tokens" (browser execution itself is outside the gate set; `palette_commands` itself
//! is proven natively in `src/console/mod.rs` and `crates/console-core`'s own tests). This
//! file mirrors `tests/console_shell_periphery.rs`'s real-socket harness verbatim (see that
//! file's own doc comment for why an in-process `route(...)` call is not enough on its own).
//!
//! `dash` compiles on BOTH the default and `--no-default-features` lanes (nothing here is
//! feature-gated), so these tests run in both. No reference to any external tool or
//! project; hyphens, never em dashes.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use rigger::contextgraph::{CallGraph, Direction, Graph};
use rigger::dash::{self, DashInputs, InstanceView};

fn get_raw(target: &str) -> Vec<u8> {
    let provider = |_instance: Option<&str>| -> Result<DashInputs, String> {
        panic!("the console shell reads no run-scoped store input")
    };
    let graph_provider = |_instance: Option<&str>| -> Graph {
        panic!("the console shell opens no whole-graph projection")
    };
    let calls_provider =
        |_instance: Option<&str>,
         _seeds: &[String],
         _dir: Direction,
         _depth: i64,
         _floor: &str|
         -> CallGraph { panic!("the console shell opens no calls projection") };
    let instances_provider =
        || -> Vec<InstanceView> { panic!("the console shell reads no instance registry") };

    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind an ephemeral loopback port");
    let addr = listener.local_addr().expect("learn the bound port");
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

fn served_console_body() -> String {
    let raw = get_raw("/console");
    let sep = b"\r\n\r\n";
    let idx = raw
        .windows(sep.len())
        .position(|w| w == sep)
        .expect("a served HTTP response has a header/body terminator");
    std::str::from_utf8(&raw[idx + sep.len()..])
        .expect("the console shell is UTF-8 HTML")
        .to_string()
}

/// THE PALETTE's own markup: a dialog carrying a filter input and a results list, hidden
/// until opened, wired to the header's existing palette button (`src/console.html`'s own
/// `palbtn`, already present since criterion 1 - see `p94-u94c1-shell-and-fonts-impl`'s own
/// note that it deliberately left the dialog markup for this criterion).
#[test]
fn the_served_console_page_carries_the_palette_dialog_markup() {
    let body = served_console_body();
    for needle in [
        "id=\"paletteOverlay\"",
        "id=\"paletteInput\"",
        "id=\"paletteList\"",
        "id=\"palbtn\"",
    ] {
        assert!(
            body.contains(needle),
            "missing {needle:?} in the served console page: {body}"
        );
    }
}

/// THE PALETTE opens on Ctrl-K and Cmd-K (Design: "`Ctrl-K` and `Cmd-K` open the
/// palette"), checked ahead of the position model's own input-tag focus guard so the
/// chorded shortcut opens the palette regardless of what currently has focus.
#[test]
fn the_served_console_page_opens_the_palette_on_ctrl_k_and_cmd_k() {
    let body = served_console_body();
    for needle in ["e.metaKey || e.ctrlKey", "\"k\"", "openPalette("] {
        assert!(
            body.contains(needle),
            "missing {needle:?} in the served console page: {body}"
        );
    }
}

/// THE PALETTE's entries come from the core's own `palette_commands` op (Design: "its
/// entries come from `palette_commands` in the core"), fetched through the SAME `callOp`
/// loader the position model already uses - never a second, page-side command list.
#[test]
fn the_served_console_page_fetches_entries_from_the_core_palette_commands_op() {
    let body = served_console_body();
    assert!(
        body.contains("callOp(\"palette_commands\""),
        "the served console page must call the core's palette_commands op: {body}"
    );
}

/// THE PALETTE filters as the person types (Design: "filtered as the person types"),
/// wired to the search input's own `input` event.
#[test]
fn the_served_console_page_filters_the_palette_as_typed() {
    let body = served_console_body();
    for needle in ["addEventListener(\"input\"", "filterPalette("] {
        assert!(
            body.contains(needle),
            "missing {needle:?} in the served console page: {body}"
        );
    }
}

/// THE PALETTE runs the highlighted entry on Enter and closes on Escape (Design: "`Enter`
/// runs the highlighted one, `Escape` closes").
#[test]
fn the_served_console_page_runs_on_enter_and_closes_on_escape() {
    let body = served_console_body();
    for needle in [
        "\"Enter\"",
        "runHighlightedPaletteCommand(",
        "\"Escape\"",
        "closePalette(",
    ] {
        assert!(
            body.contains(needle),
            "missing {needle:?} in the served console page: {body}"
        );
    }
}

/// THE PALETTE's five kinds of entry each resolve to a real action on the page: a view
/// (the existing tab click handler), a unit's courtroom, an agent, jump to live and
/// replay from start - the latter two reusing this SAME module's own `jumpToLive`/
/// `goTo`/`togglePlay` (u94c3's cursor and replay functions), never a second copy of
/// them (Design: "jumps to a view, a unit's courtroom, an agent or a round, to live, or
/// to a replay from the start").
#[test]
fn the_served_console_page_resolves_every_palette_entry_kind_to_a_real_action() {
    let body = served_console_body();
    for needle in [
        "cmd.kind === \"view\"",
        "cmd.kind === \"courtroom\"",
        "cmd.kind === \"agent\"",
        "cmd.kind === \"live\"",
        "cmd.kind === \"replay\"",
        "jumpToLive()",
    ] {
        assert!(
            body.contains(needle),
            "missing {needle:?} in the served console page: {body}"
        );
    }
    // Replay from start reuses goTo/togglePlay over the earliest known position - never a
    // second, independently invented replay path.
    assert!(
        body.contains("STATE.positions[0]") && body.contains("togglePlay()"),
        "replay from start must reuse the position model's own goTo/togglePlay: {body}"
    );
}

/// The digits 0-6 switch views (Design §6.9, the same "THE PALETTE AND KEYS" paragraph
/// as Ctrl-K/Cmd-K), in the tab bar's own left-to-right order.
#[test]
fn the_served_console_page_switches_views_on_digits_0_to_6() {
    let body = served_console_body();
    assert!(
        body.contains("/^[0-6]$/"),
        "the digit keys 0-6 must switch views: {body}"
    );
}
