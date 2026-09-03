//! Periphery for spec 77's Option-returning `crate::liveness::marker_path` contract at the MCP
//! `rigger_activity` seam (`src/mcpserver.rs::tool_activity`) - the one consumer of that changed
//! contract this unit's own diff left with ZERO test coverage of the `Some(path)` arm.
//!
//! WHY THIS GAP SURVIVED. Round 7 (spec 77 Design `d77-injective-scratch-naming`) changed
//! `crate::liveness::marker_path`'s return type to `Option<PathBuf>` and updated every call site
//! to skip a `None` rather than stat a fabricated path - `tool_activity`'s liveness-age loop
//! included (`mcpserver.rs`: `let Some(path) = crate::liveness::marker_path(...) else { continue
//! };`). But the SOLE existing test that drives `rigger_activity`,
//! `mcpserver.rs::activity_tool_presents_the_live_per_agent_view`, builds its `Server` with
//! `.with_progress(&progress, "")` - an EMPTY scratch root - so `tool_activity`'s own
//! `if !self.scratch_root.is_empty()` guard short-circuits before the changed line is ever
//! reached, in either direction. This file drives the `Some(path)` arm with a REAL, non-empty
//! scratch root and a REAL on-disk marker file, calling ONLY the crate's public API
//! (`rigger::mcpserver::Server`, `rigger::liveness::marker_path`, ...) from this external test
//! crate - the same "periphery calls the library's public surface directly" shape
//! `tests/cli.rs` already uses for `rigger::mcpserver::emit_event` - so the changed line's
//! marker-stat/age-insert code, not just its signature, is proven to still work against a real
//! file.
//!
//! WHY NOT A `rigger serve` SUBPROCESS (the usual periphery pattern for this MCP surface, see
//! `tests/workflow_driver_resolved_model_periphery.rs`). Investigated and rejected: under the
//! WORKFLOW driver `rigger serve` composes `Server` with (`src/driver/workflow.rs::Driver`), a
//! parked spawn is tracked purely in an in-memory queue and NEVER appended to the run's event
//! log as a `TYPE_SPAWN_REQUESTED` event - only the REPLAY driver (`src/driver/replay.rs`,
//! `rigger step`'s driver) parks that way. `tool_activity`'s frontier is
//! `spawn::step_result(run_events)?.wave`, which folds exactly those `TYPE_SPAWN_REQUESTED`
//! events - so under a real `rigger serve` process the frontier that array is built from is
//! structurally always empty, confirmed empirically (a subprocess-driven version of this test,
//! written first, panicked with `rigger_activity must report the in-flight spawn ...; got []`
//! after a real `rigger_next` queued the spawn). Seeding a `TYPE_SPAWN_REQUESTED` event into the
//! store directly WHILE a real `rigger serve` subprocess runs would not be any more faithful to
//! production than this file's direct approach - both routes fabricate a state the workflow
//! driver alone never produces - so the subprocess adds process/wire overhead with no added
//! realism for this specific arm. This driver-pairing gap (`rigger_activity`'s frontier query
//! and the workflow driver's parking model never actually meet in production) is itself a
//! pre-existing, spec-14-territory characteristic, orthogonal to spec 77's diff - out of scope
//! here, noted only as the reason this file's approach differs from the sibling MCP periphery
//! file's subprocess pattern.
//!
//! NOT OWNED HERE:
//! - `crate::progress::consolidate`'s own age arithmetic - a different file, no part of this
//!   unit's diff, already unit-tested directly in `src/progress.rs`.
//! - The `None` arm at this same call site: every id in `spawn::step_result`'s frontier is
//!   `spawn::spawn_id`-derived (always `{unit}/{role}#{n}`, never empty), so `marker_path` can
//!   never actually return `None` there in practice - unlike `reclaim_spawn_scratch`'s
//!   `spawn_id.split('/').next()` unit extraction (main.rs), which DOES reach the degenerate
//!   shape via the `rigger result <id>` positional's own lack of format validation and is
//!   exactly what the round 3/5/7 regression tests in `tests/cli.rs` already close end to end.

use std::io::Cursor;
use std::path::Path;
use std::time::{Duration, SystemTime};

use serde_json::Value;

use rigger::driver::workflow::Driver;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Event, EventStore, ExpectedRevision, Filter};
use rigger::mcpserver::Server;
use rigger::sidecar::Sidecar;
use rigger::spawn::SpawnRequest;

/// Plant a real marker file at `path`, backdated by `secs_ago` seconds - mirrors `tests/cli.rs`'s
/// `plant_stale_marker`, generalized to an arbitrary (non-stale) age so the test can assert the
/// consolidated view reports roughly that age rather than merely "present".
fn plant_marker(path: &Path, secs_ago: u64) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"heartbeat").unwrap();
    let when = SystemTime::now() - Duration::from_secs(secs_ago);
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(when)
        .unwrap();
}

/// The `Some(path)` arm of `tool_activity`'s liveness-age loop (`mcpserver.rs`, changed by this
/// unit's round-7 diff to skip a `None` from the now-`Option`-returning `marker_path`): a real
/// on-disk marker, planted at the exact path `crate::liveness::marker_path` resolves for an
/// in-flight spawn, must reach `rigger_activity`'s `liveness_age_s` field through the real
/// `Server::run` JSON-RPC handling - proving the changed call site still wires a real marker's
/// age into the consolidated view against a genuine, non-empty scratch root, not merely that it
/// compiles or that the empty-scratch-root short-circuit is well-formed.
#[test]
fn rigger_activity_reports_a_real_markers_liveness_age_over_a_non_empty_scratch_root() {
    let store = Store::open(":memory:").unwrap();
    let progress = Store::open(":memory:").unwrap();
    let driver = Driver::new();
    let peers = Sidecar::start(&store, 0, Filter::default()).unwrap();

    // A run: a unit started, its implementer parked (in-flight, no result yet) - the exact
    // frontier shape `mcpserver.rs::activity_tool_presents_the_live_per_agent_view` seeds,
    // since only THIS shape (a persisted `TYPE_SPAWN_REQUESTED` event) is what
    // `spawn::step_result` folds into `tool_activity`'s frontier.
    let run_id = rigger::run::ensure_started(&store, &["crit".to_string()]).unwrap();
    store
        .append(
            "run",
            ExpectedRevision::Any,
            &[Event::new("UnitStarted", b"{\"id\":\"u\"}".to_vec())],
        )
        .unwrap();
    let req = SpawnRequest::new("u", "u", "implementer", 0, "do it");
    store
        .append("run", ExpectedRevision::Any, &[req.to_event().unwrap()])
        .unwrap();

    // A REAL, non-empty scratch root (a tempdir), unlike the sibling in-process test's "" - the
    // one change that reaches the changed `Some(path)` arm at all. A real marker planted at the
    // EXACT path `crate::liveness::marker_path` resolves for this run/spawn, backdated a known
    // amount so the reported age can be checked against a real bound, not just presence.
    let scratch_dir = tempfile::tempdir().unwrap();
    let scratch_root = scratch_dir.path().to_str().unwrap().to_string();
    let marker = rigger::liveness::marker_path(&scratch_root, &run_id, &req.id)
        .expect("a real, non-empty spawn id must resolve a marker path");
    let secs_ago = 5u64;
    plant_marker(&marker, secs_ago);

    let server =
        Server::new(&driver, &store, "run", &peers).with_progress(&progress, &scratch_root);
    let input = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_activity","arguments":{}}}"#;
    let mut output = Vec::new();
    server.run(Cursor::new(input), &mut output).unwrap();

    let resp: Value = serde_json::from_str(String::from_utf8(output).unwrap().trim()).unwrap();
    let view = &resp["result"]["structuredContent"];
    assert_eq!(
        view.as_array().map(|a| a.len()),
        Some(1),
        "one in-flight agent; got {view}"
    );
    assert_eq!(view[0]["id"], req.id);

    let age = view[0]["liveness_age_s"].as_u64().unwrap_or_else(|| {
        panic!(
            "rigger_activity must report a numeric liveness_age_s for a real, freshly-stat-able \
             marker under a non-empty scratch root (the changed Some(path) arm must reach and \
             stat it); got view: {view}"
        )
    });
    // A loose bound, not an exact match: the age is real wall-clock elapsed between the plant
    // and the read, not a mocked clock. `consolidate`'s own exact age arithmetic is already
    // pinned at the pure-function level in `src/progress.rs`; this only proves the real marker
    // on the real filesystem, at the real resolved path, threads through the real tool.
    assert!(
        (secs_ago.saturating_sub(2)..secs_ago + 60).contains(&age),
        "liveness_age_s must reflect the real marker's age (~{secs_ago}s, allowing scheduling \
         slack); got {age}"
    );
}

/// Sibling of the above, proving the OTHER real-disk-I/O half: when `crate::liveness::
/// marker_path` resolves a path but NO file exists there yet (the spawn parked but has not
/// touched its heartbeat marker), `tool_activity` must omit `liveness_age_s` rather than error
/// or fabricate a zero - the SAME "a spawn with NO marker is left alone" idiom `sweep`'s own doc
/// comment states, now proven at THIS call site under a real, non-empty scratch root (the
/// existing in-process test's empty-scratch-root shortcut cannot distinguish this from the
/// no-scratch-root-at-all case, since both take the outer `if` to `false`/skip for different
/// reasons).
#[test]
fn rigger_activity_omits_liveness_age_when_no_marker_file_exists_yet() {
    let store = Store::open(":memory:").unwrap();
    let progress = Store::open(":memory:").unwrap();
    let driver = Driver::new();
    let peers = Sidecar::start(&store, 0, Filter::default()).unwrap();

    rigger::run::ensure_started(&store, &["crit".to_string()]).unwrap();
    store
        .append(
            "run",
            ExpectedRevision::Any,
            &[Event::new("UnitStarted", b"{\"id\":\"u\"}".to_vec())],
        )
        .unwrap();
    let req = SpawnRequest::new("u", "u", "implementer", 0, "do it");
    store
        .append("run", ExpectedRevision::Any, &[req.to_event().unwrap()])
        .unwrap();

    // A real, non-empty scratch root with NOTHING planted under it - `agent-live/` (and
    // everything below it) simply does not exist on disk yet.
    let scratch_dir = tempfile::tempdir().unwrap();
    let scratch_root = scratch_dir.path().to_str().unwrap().to_string();

    let server =
        Server::new(&driver, &store, "run", &peers).with_progress(&progress, &scratch_root);
    let input = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_activity","arguments":{}}}"#;
    let mut output = Vec::new();
    server.run(Cursor::new(input), &mut output).unwrap();

    let resp: Value = serde_json::from_str(String::from_utf8(output).unwrap().trim()).unwrap();
    let view = &resp["result"]["structuredContent"];
    assert_eq!(
        view.as_array().map(|a| a.len()),
        Some(1),
        "one in-flight agent; got {view}"
    );
    assert_eq!(view[0]["id"], req.id);
    assert!(
        view[0].get("liveness_age_s").is_none(),
        "no marker file exists yet, so liveness_age_s must be omitted (never a fabricated 0 or \
         an error); got view: {view}"
    );
}
