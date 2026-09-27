//! Periphery (contract / API / integration) test for spec 94 criterion 3, THE POSITION
//! MODEL - proven at the level Design's own Notes name for a page-side criterion in this
//! spec: "the served page's source is asserted to call those ops and to carry the mock's
//! markup and tokens" (browser execution itself is outside the gate set; the logic these
//! calls drive - `fold_at`, `scrub_track` - is proven natively in `src/console/mod.rs` and
//! `crates/console-core`'s own tests). This file mirrors `tests/console_shell_periphery.rs`'s
//! real-socket harness verbatim (see that file's own doc comment for why an in-process
//! `route(...)` call is not enough on its own).
//!
//! `dash` compiles on BOTH the default and `--no-default-features` lanes (nothing here is
//! feature-gated), so these tests run in both. No reference to any external tool or
//! project; hyphens, never em dashes.

mod common;

use common::served::assert_console_body_carries;
use common::served::{assert_served_console_page_carries, served_console_body};

rigger::test_cases! {
    /// THE POSITION MODEL loads the wasm core and drives it through the three fold ops named
    /// verbatim in the addendum's own data-plane diagram: `fold_reset(events)`, `fold_push(e)`,
    /// `fold_at(N)`.
    the_served_console_page_loads_the_wasm_core_and_calls_the_three_fold_ops:
        assert_served_console_page_carries(&["/console/core.wasm", "console_alloc", "console_call", "console_free", "fold_reset", "fold_push", "fold_at", "scrub_track"]);
}

rigger::test_cases! {
    /// THE POSITION MODEL loads the snapshot into `fold_reset` and follows the live stream with
    /// `since=` into `fold_push` (Design: "the page loads the snapshot into the core
    /// (`fold_reset`), pushes stream events (`fold_push`)").
    the_served_console_page_fetches_the_snapshot_and_follows_the_stream:
        assert_served_console_page_carries(&["/api/console/snapshot", "/api/console/stream", "since=", "EventSource("]);
}

/// THE POSITION MODEL's replay/keys (Design: "the play button replays from the cursor at
/// five events per second and stops at the head; space toggles play; the left and right
/// arrows step one event; `End` jumps to live").
#[test]
fn the_served_console_page_wires_replay_and_keyboard_controls() {
    let body = served_console_body();
    assert_console_body_carries(
        &body,
        &[
            "ArrowLeft",
            "ArrowRight",
            "\"End\"",
            "setInterval(",
            "1000 / 5",
        ],
    );
    // Space toggles play - proven via either the printable-space key or its named code,
    // never requiring a specific one of the two equally valid DOM spellings.
    assert!(
        body.contains("e.key === \" \"") || body.contains("\"Space\""),
        "the space key must toggle play/pause: {body}"
    );
}

rigger::test_cases! {
    /// THE POSITION MODEL's URL hash (Design: "The URL hash carries the view, the selection and
    /// the position (`#/theater?at=N`, ...) and is restored on load, so a moment is a link").
    the_served_console_page_restores_and_updates_the_url_hash:
        assert_served_console_page_carries(&["location.hash", "history.replaceState", "?at="]);
}

/// THE POSITION MODEL's retained-window recovery (CONSTRAINTS WALK, "Stream drop": "a gap
/// beyond the server's retained window triggers a snapshot re-fetch";
/// `adj-u94c3-r2-verdict-reject-retained-window-gap-refetch`). A real `EventSource` sets
/// `readyState` to `CLOSED` only for a DEFINITIVE server refusal (`write_retained_window_
/// gone`, `src/dash.rs`'s own `410`) - never for an ordinary dropped connection, which it
/// retries on its own with no page-side call at all (`readyState` stays `CONNECTING` for
/// that case, and this page must do nothing then, or every routine reconnect would refetch
/// the whole snapshot). `onerror` must act on exactly that `CLOSED` signal by re-fetching
/// the snapshot, folding it fresh, and reopening the stream - never the bare no-op it was
/// before this fix.
#[test]
fn the_served_console_page_recovers_from_a_closed_stream_by_refetching_the_snapshot() {
    let body = served_console_body();
    assert!(
        !body.contains("source.onerror = function () {};"),
        "onerror must no longer be a bare no-op: {body}"
    );
    for needle in ["EventSource.CLOSED", "/api/console/snapshot", "fold_reset"] {
        assert!(
            body.contains(needle),
            "missing {needle:?} in the served console page: {body}"
        );
    }
}
