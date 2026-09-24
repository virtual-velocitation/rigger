//! Periphery for spec 104 criterion 2 (THE STREAM IS THE RECORD): drives
//! `impl AgentDriver for Driver` end to end through the real `spawn()` trait method - a
//! real subprocess, a real stdout pipe, two real (in-memory) stores - exercising exactly
//! the boundary the implementer's own in-file `mod tests` structurally cannot: everything
//! `driver::claude_code`'s own pure-function tests (`stream_path`, `api_retry_line`,
//! `spawn_result_from`) prove on a hand-built `serde_json::Value`, never wired through a
//! live child process, a liveness marker on real disk, a raw-stream file, or the actual
//! `progress_store`/`spawn_store` writers.
//!
//! The fixture agent (`tests/fixtures/claude-code-stream-agent.sh`) replays a stream
//! recorded from - and grounded on the exact field names of - a real
//! `claude -p --output-format stream-json --input-format stream-json --verbose` session
//! (`tests/fixtures/claude-code-stream-success.jsonl`), so this proves the reader against
//! the real wire shape, not a shape this unit invented for itself.
//!
//! NOT OWNED HERE: argv/cwd/env/the open launch record (criterion 1, already proven by
//! `claude_code_launch_wire_periphery.rs`); failure classification, relaunch, and the
//! composition-root swap's BEHAVIOR under an actual failing session (criterion 5, not
//! landed on this branch); the spawn-bound MCP server, the write guard and the
//! StopFailure hooks (criteria 3 and 4, separate units, already landed independently).

mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use rigger::conductor::{AgentDriver, SpawnOpts};
use rigger::config::AgentDef;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore};
use rigger::liveness;
use rigger::progress::{AgentProgress, TYPE_AGENT_PROGRESS};
use rigger::spawn::{self, TYPE_SPAWN_RESULT};

fn fixture_bin() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/claude-code-stream-agent.sh")
        .to_string_lossy()
        .into_owned()
}

/// A fresh `Driver` wired to two independent in-memory stores and a scratch root under
/// the test's own tempdir - so every test gets its own isolated scratch tree and stores,
/// never sharing state with a sibling test.
struct Fixture {
    progress_store: Store,
    run_store: Store,
    scratch_root: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        Fixture {
            progress_store: Store::open(":memory:").unwrap(),
            run_store: Store::open(":memory:").unwrap(),
            scratch_root: tempfile::tempdir().unwrap(),
        }
    }

    fn driver(&self) -> rigger::driver::claude_code::Driver<'_> {
        rigger::driver::claude_code::Driver {
            bin: fixture_bin(),
            rigger_bin: "rigger".to_string(),
            progress_store: &self.progress_store,
            run_store: &self.run_store,
            scratch_root: self.scratch_root.path().to_string_lossy().into_owned(),
            stop_grace: std::time::Duration::from_secs(30),
        }
    }
}

fn opts(id: &str) -> SpawnOpts {
    SpawnOpts {
        id: id.to_string(),
        run_id: "run-1".to_string(),
        ..Default::default()
    }
}

#[test]
fn spawn_reads_the_recorded_stream_to_a_real_result() {
    let fx = Fixture::new();
    let o = opts("u104-stream/implementer#0");
    let emit = |_: &str, _: serde_json::Value| Ok(());

    let result = fx
        .driver()
        .spawn(&AgentDef::default(), "do the thing", &o, &emit)
        .expect("the recorded stream ends in a result");

    assert_eq!(result.output, "done: the answer is 42");
    assert_eq!(result.resolved_model, "claude-sonnet-4-5-20250929");
}

#[test]
fn spawn_records_the_result_in_the_run_store_with_its_full_meta() {
    let fx = Fixture::new();
    let o = opts("u104-stream/implementer#0");
    let emit = |_: &str, _: serde_json::Value| Ok(());

    fx.driver()
        .spawn(&AgentDef::default(), "do the thing", &o, &emit)
        .unwrap();

    let events = fx
        .run_store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap();
    let results: Vec<_> = events
        .iter()
        .filter(|e| e.type_ == TYPE_SPAWN_RESULT)
        .collect();
    assert_eq!(results.len(), 1, "exactly one SpawnResult landed");
    let res = spawn::SpawnResult::from_event(results[0]).unwrap();
    assert_eq!(res.id, "u104-stream/implementer#0");
    assert_eq!(res.output, "done: the answer is 42");
    assert!(!res.is_error());
    assert_eq!(res.resolved_model(), "claude-sonnet-4-5-20250929");
    assert_eq!(
        res.meta["session_id"],
        "11111111-1111-4111-8111-111111111111"
    );
    assert_eq!(res.meta["usage"]["input"], 100);
    assert_eq!(res.meta["usage"]["output"], 50);
    assert_eq!(res.meta["usage"]["cache_creation"], 20);
    assert_eq!(res.meta["usage"]["cache_read"], 10);
    assert_eq!(res.meta["turns"], 3);
    assert_eq!(res.meta["cost_usd"], 0.0456);
    // One `system/permission_denied` line in the fixture.
    assert_eq!(res.meta["permission_denials"], 1);
}

#[test]
fn a_relaunch_never_double_records_the_result() {
    // CONSTRAINTS WALK: "Relaunch - a new session id and the next launch ordinal; the
    // log holds one result." record_result_if_absent is what enforces that - prove it
    // by calling spawn() twice for the SAME id and asserting the run store still holds
    // exactly one SpawnResult.
    let fx = Fixture::new();
    let o = opts("u104-stream/implementer#0");
    let emit = |_: &str, _: serde_json::Value| Ok(());

    fx.driver()
        .spawn(&AgentDef::default(), "first", &o, &emit)
        .unwrap();
    fx.driver()
        .spawn(&AgentDef::default(), "second", &o, &emit)
        .unwrap();

    let events = fx
        .run_store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap();
    let results: Vec<_> = events
        .iter()
        .filter(|e| e.type_ == TYPE_SPAWN_RESULT)
        .collect();
    assert_eq!(
        results.len(),
        1,
        "the second spawn's result never clobbers the first"
    );
}

#[test]
fn spawn_touches_the_liveness_marker_and_persists_the_raw_stream() {
    let fx = Fixture::new();
    let o = opts("u104-stream/implementer#0");
    let emit = |_: &str, _: serde_json::Value| Ok(());
    let scratch_root = fx.scratch_root.path().to_string_lossy().into_owned();

    fx.driver()
        .spawn(&AgentDef::default(), "do the thing", &o, &emit)
        .unwrap();

    let marker = liveness::marker_path(&scratch_root, &o.run_id, &o.id).unwrap();
    let meta = std::fs::metadata(&marker).expect("the liveness marker was touched");
    let age = SystemTime::now()
        .duration_since(meta.modified().unwrap())
        .unwrap();
    assert!(age.as_secs() < 30, "marker just touched: {age:?}");

    let raw = std::fs::read_to_string(
        Path::new(&scratch_root)
            .join("agent-stream")
            .join("run-1")
            .join("u104-stream_2fimplementer_230.0.jsonl"),
    )
    .expect("the raw stream transcript was persisted");
    assert_eq!(
        raw.lines().count(),
        6,
        "every recorded line landed verbatim: {raw:?}"
    );
    assert!(raw.contains("\"subtype\":\"init\""));
    assert!(raw.contains("done: the answer is 42"));
}

#[test]
fn spawn_turns_api_retry_and_the_unparseable_line_into_progress_reports() {
    let fx = Fixture::new();
    let o = opts("u104-stream/implementer#0");
    let emit = |_: &str, _: serde_json::Value| Ok(());

    fx.driver()
        .spawn(&AgentDef::default(), "do the thing", &o, &emit)
        .unwrap();

    let events = fx
        .progress_store
        .read_stream(rigger::progress::STREAM, 0, Direction::Forward)
        .unwrap();
    let activities: Vec<String> = events
        .iter()
        .filter(|e| e.type_ == TYPE_AGENT_PROGRESS)
        .map(|e| {
            serde_json::from_slice::<AgentProgress>(&e.data)
                .unwrap()
                .activity
        })
        .collect();

    assert!(
        activities
            .iter()
            .any(|a| a.contains("rate_limit") && a.contains("618")),
        "activities: {activities:?}"
    );
    assert!(
        activities.iter().any(|a| a.contains("permission denied")),
        "activities: {activities:?}"
    );
    assert!(
        activities
            .iter()
            .any(|a| a.contains("unparseable") && a.contains("not json at all")),
        "activities: {activities:?}"
    );
}

#[test]
fn spawn_errors_loudly_when_the_stream_ends_with_no_result() {
    // The reader's OWN correctness, distinct from failure CLASSIFICATION (criterion 5,
    // not this one's): a stream that never produces a `result` must never read as a
    // silent success.
    let fx = Fixture::new();
    let bin = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/claude-code-stream-no-result-agent.sh")
        .to_string_lossy()
        .into_owned();
    let driver = rigger::driver::claude_code::Driver { bin, ..fx.driver() };
    let o = opts("u104-stream/implementer#0");
    let emit = |_: &str, _: serde_json::Value| Ok(());

    let err = driver
        .spawn(&AgentDef::default(), "do the thing", &o, &emit)
        .expect_err("a stream with no result must not read as success");
    assert!(err.0.contains("u104-stream/implementer#0"), "{}", err.0);

    // No SpawnResult was recorded either.
    let events = fx
        .run_store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap();
    assert!(!events.iter().any(|e| e.type_ == TYPE_SPAWN_RESULT));
}

#[test]
fn spawn_ignores_the_emit_callback_the_agent_reports_its_own_decisions_live() {
    // Unlike the cli driver, this host's agent records decisions LIVE through its own
    // bound MCP server (criterion 3) - `spawn()` bridges nothing from stdout, so `emit`
    // is never called.
    let fx = Fixture::new();
    let o = opts("u104-stream/implementer#0");
    let calls = std::sync::Mutex::new(0u32);
    let emit = |_: &str, _: serde_json::Value| {
        *calls.lock().unwrap() += 1;
        Ok(())
    };

    fx.driver()
        .spawn(&AgentDef::default(), "do the thing", &o, &emit)
        .unwrap();

    assert_eq!(*calls.lock().unwrap(), 0);
}

#[test]
fn spawn_propagates_a_launch_failure_never_reading_a_stream_that_never_started() {
    // `spawn()` (the `AgentDriver` method every other test in this file drives) is
    // `launch()` composed with `read_stream()` - the implementer's own `mod tests`
    // proves `launch()`'s failure in isolation (a nonexistent `bin`); this proves the
    // SAME failure surfaces through the composed call, not just the half-call.
    let fx = Fixture::new();
    let bin = "/definitely/does/not/exist/claude-code-xyz".to_string();
    let driver = rigger::driver::claude_code::Driver { bin, ..fx.driver() };
    let o = opts("u104-stream/implementer#0");
    let emit = |_: &str, _: serde_json::Value| Ok(());

    let err = driver
        .spawn(&AgentDef::default(), "do the thing", &o, &emit)
        .expect_err("spawning a nonexistent binary must fail through spawn(), not just launch()");
    assert!(err.0.contains("u104-stream/implementer#0"), "{}", err.0);

    // No SpawnResult landed either - a launch that never started a process never
    // produced a stream to read a result from.
    let events = fx
        .run_store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap();
    assert!(!events.iter().any(|e| e.type_ == TYPE_SPAWN_RESULT));
}

/// Restores the process's current directory on drop, however the guarded scope exits -
/// used only by [`spawn_with_an_empty_scratch_root_never_writes_relative_to_cwd`], the
/// one test in this file that mutates this process-global resource.
struct CwdGuard(PathBuf);

impl CwdGuard {
    fn enter(dir: &Path) -> Self {
        let original = std::env::current_dir().expect("read the current directory");
        std::env::set_current_dir(dir).expect("enter the throwaway directory");
        CwdGuard(original)
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}

#[test]
#[serial_test::serial(claude_code_stream_cwd)]
fn spawn_with_an_empty_scratch_root_never_writes_relative_to_cwd() {
    // `Driver::scratch_root`'s own doc: "Empty disables both [the liveness marker and
    // the raw stream file] - no scratch root configured / a test that does not care."
    // PROVE it rather than trust it: every OTHER test in this file supplies a real
    // scratch root, so this is the one place that exercises the degenerate case a
    // caller who does not care is explicitly invited to pass. Run `spawn()` from
    // inside a throwaway directory (never the real worktree; `#[serial]` because this
    // mutates the process-global cwd, matching this crate's own convention - see
    // Cargo.toml's `serial_test` comment - for cwd-sensitive tests) and assert neither
    // `agent-live` nor `agent-stream` appears anywhere under it: an empty scratch root
    // must be a true no-op, never a RELATIVE path that scatters files into whatever
    // directory the caller happened to be running in (which, for a real `rigger run`
    // invocation, would be the operator's own repository checkout).
    let throwaway = tempfile::tempdir().unwrap();
    let _cwd_guard = CwdGuard::enter(throwaway.path());

    let fx = Fixture::new();
    let driver = rigger::driver::claude_code::Driver {
        bin: fixture_bin(),
        rigger_bin: "rigger".to_string(),
        progress_store: &fx.progress_store,
        run_store: &fx.run_store,
        scratch_root: String::new(),
        stop_grace: std::time::Duration::from_secs(30),
    };
    let o = opts("u104-stream/implementer#0");
    let emit = |_: &str, _: serde_json::Value| Ok(());

    driver
        .spawn(&AgentDef::default(), "do the thing", &o, &emit)
        .expect("an empty scratch root must still let the spawn succeed");

    assert!(
        !throwaway.path().join("agent-live").exists(),
        "an empty scratch_root must not create a liveness marker relative to cwd"
    );
    assert!(
        !throwaway.path().join("agent-stream").exists(),
        "an empty scratch_root must not create a stream transcript relative to cwd"
    );
}

#[test]
fn spawn_records_the_mcp_connection_status_from_system_init_as_a_progress_line() {
    // `system/init`'s `mcp_servers` array is "noted for the record" (Design) - this
    // file's other progress-line assertions cover api_retry/denied/unparseable but
    // never this branch of the same match arm; close it.
    let fx = Fixture::new();
    let o = opts("u104-stream/implementer#0");
    let emit = |_: &str, _: serde_json::Value| Ok(());

    fx.driver()
        .spawn(&AgentDef::default(), "do the thing", &o, &emit)
        .unwrap();

    let events = fx
        .progress_store
        .read_stream(rigger::progress::STREAM, 0, Direction::Forward)
        .unwrap();
    let activities: Vec<String> = events
        .iter()
        .filter(|e| e.type_ == TYPE_AGENT_PROGRESS)
        .map(|e| {
            serde_json::from_slice::<AgentProgress>(&e.data)
                .unwrap()
                .activity
        })
        .collect();

    assert!(
        activities
            .iter()
            .any(|a| a.contains("mcp servers") && a.contains("rigger") && a.contains("connected")),
        "activities: {activities:?}"
    );
}

// ---- adj-u104-stream REQUIRED FIX 1: read_stream reaps the child on EVERY early return,
// through one path (`dash::ReapedChild`, spec 104 round-4 REQUIRED FIX 2), not a
// per-branch patch ----

#[cfg(unix)]
#[test]
fn spawn_reaps_the_child_on_a_mid_stream_read_error() {
    // A GENUINE mid-stream read error (invalid UTF-8 on stdout, not merely EOF or a
    // missing `result`) must still end the child through its own handle before spawn()'s
    // `Err` propagates - the exact defect the rejected round's adjudication named: three
    // early returns inside `read_stream` skipped the reap `launch()` itself already
    // proves (`claude_code_launch_wire_periphery.rs::launch_reaps_the_child_when_the_stdin_write_fails`).
    // Proven the same way that test proves launch()'s half: a REAL pid, written by the
    // fixture itself before it ever emits invalid UTF-8, read back independently of the
    // `Child` handle `read_stream` already reaped, via `common::is_alive` - never the
    // handle that did the reaping.
    let fx = Fixture::new();
    let bin = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/claude-code-invalid-utf8-agent.sh")
        .to_string_lossy()
        .into_owned();
    let driver = rigger::driver::claude_code::Driver { bin, ..fx.driver() };
    let pid_file = fx.scratch_root.path().join("agent.pid");
    let mut o = opts("u104-stream/implementer#0");
    o.env = vec![(
        "RIGGER_TEST_INVALID_UTF8_PID_FILE".to_string(),
        pid_file.to_string_lossy().into_owned(),
    )];
    let emit = |_: &str, _: serde_json::Value| Ok(());

    let err = driver
        .spawn(&AgentDef::default(), "do the thing", &o, &emit)
        .expect_err("invalid utf-8 on the stream must surface as a read error, not a result");
    assert!(err.0.contains("read agent stream"), "{}", err.0);

    let pid_text = std::fs::read_to_string(&pid_file)
        .expect("the fixture recorded its pid before writing invalid utf-8");
    let pid: u32 = pid_text
        .trim()
        .parse()
        .unwrap_or_else(|e| panic!("pid file {pid_text:?} did not parse: {e}"));
    assert!(
        !common::is_alive(pid),
        "child pid {pid} must not survive a mid-stream read error - it must be reaped \
         (through dash::ReapedChild), never leaked"
    );

    // No SpawnResult landed either - a stream that errored mid-read never reached a
    // `result` message.
    let events = fx
        .run_store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap();
    assert!(!events.iter().any(|e| e.type_ == TYPE_SPAWN_RESULT));
}

// ---- adj-u104-stream REQUIRED FIX 2: stderr is drained concurrently with stdout, so a
// chatty agent can never deadlock the host ----

#[test]
fn spawn_drains_stderr_concurrently_so_a_stderr_flood_cannot_deadlock_the_host() {
    // sdet reproduced this exactly against the compiled driver in the rejected round: a
    // real agent that writes more than one pipe buffer of stderr (Linux default 64KiB)
    // before its first stdout line must never deadlock this host, which - before this
    // fix - read stdout and stderr on the SAME thread in sequence, so the child's stderr
    // write() and this host's stdout read() would block on each other forever.
    //
    // Bounded via a channel + `recv_timeout` (mirroring
    // `watchdog_cli_periphery.rs`'s own pattern for a possibly-blocking external process)
    // so a regression fails THIS test in a few seconds instead of hanging the whole
    // suite. Every store/driver the spawned thread touches is built INSIDE the thread
    // (never borrowed from the test's own stack), so the call is a plain 'static
    // `thread::spawn`, not a scoped one - simplest correct shape for a call this test
    // must be able to abandon (never `join`) if it times out.
    let bin = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/claude-code-stderr-flood-agent.sh")
        .to_string_lossy()
        .into_owned();
    let scratch_root = tempfile::tempdir().unwrap();
    let scratch_root = scratch_root.path().to_string_lossy().into_owned();

    let (tx, rx) = std::sync::mpsc::channel::<Result<String, String>>();
    std::thread::spawn(move || {
        let progress_store = Store::open(":memory:").unwrap();
        let run_store = Store::open(":memory:").unwrap();
        let driver = rigger::driver::claude_code::Driver {
            bin,
            rigger_bin: "rigger".to_string(),
            progress_store: &progress_store,
            run_store: &run_store,
            scratch_root,
            stop_grace: std::time::Duration::from_secs(30),
        };
        let o = opts("u104-stream/implementer#0");
        let emit = |_: &str, _: serde_json::Value| Ok(());
        let outcome = driver
            .spawn(&AgentDef::default(), "do the thing", &o, &emit)
            .map(|r| r.output)
            .map_err(|e| e.0);
        let _ = tx.send(outcome);
    });

    let outcome = rx
        .recv_timeout(Duration::from_secs(15))
        .expect("spawn() must return rather than deadlock on a stderr-flooding agent");
    assert_eq!(outcome, Ok("done: the answer is 42".to_string()));
}

// ---- adj-u104-stream round-2 REQUIRED FIX: once `result` is `Some`, a later per-line
// read error on the SAME stream must not overturn the durably-recorded success ----

#[cfg(unix)]
#[test]
fn spawn_survives_a_read_error_that_arrives_after_the_result_line() {
    // The rejected round's exact finding: `read_stream` keeps consuming stdout lines
    // after capturing `result` (by design - only the FIRST result acts, so more lines,
    // including a duplicate result, are anticipated), but a bare `?` on any LATER
    // line's read error still discarded the already-recorded success. This fixture
    // emits the real recorded result line first - durably recording it via
    // `record_result_if_absent`, exactly like every other test in this file - THEN one
    // line of invalid UTF-8: the opposite order from
    // `spawn_reaps_the_child_on_a_mid_stream_read_error`'s error-before-any-result
    // shape.
    let fx = Fixture::new();
    let bin = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/claude-code-post-result-read-error-agent.sh")
        .to_string_lossy()
        .into_owned();
    let driver = rigger::driver::claude_code::Driver { bin, ..fx.driver() };
    let pid_file = fx.scratch_root.path().join("agent.pid");
    let mut o = opts("u104-stream/implementer#0");
    o.env = vec![(
        "RIGGER_TEST_POST_RESULT_PID_FILE".to_string(),
        pid_file.to_string_lossy().into_owned(),
    )];
    let emit = |_: &str, _: serde_json::Value| Ok(());

    let result = driver
        .spawn(&AgentDef::default(), "do the thing", &o, &emit)
        .expect("a read error strictly after the result must not overturn it");
    assert_eq!(result.output, "done: the answer is 42");
    assert_eq!(result.resolved_model, "claude-sonnet-4-5-20250929");

    // Exactly one SpawnResult landed - the real one, durably recorded before the
    // later read error ever happened.
    let events = fx
        .run_store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap();
    let results: Vec<_> = events
        .iter()
        .filter(|e| e.type_ == TYPE_SPAWN_RESULT)
        .collect();
    assert_eq!(results.len(), 1, "exactly one SpawnResult landed");

    // The child is still reaped through its own handle, never leaked, even though a
    // read error - not a clean EOF - is what ended this loop.
    let pid_text = std::fs::read_to_string(&pid_file)
        .expect("the fixture recorded its pid before writing invalid utf-8");
    let pid: u32 = pid_text
        .trim()
        .parse()
        .unwrap_or_else(|e| panic!("pid file {pid_text:?} did not parse: {e}"));
    assert!(
        !common::is_alive(pid),
        "child pid {pid} must not survive a post-result read error - it must still be \
         reaped (through dash::ReapedChild), never leaked"
    );
}

// ---- adj-u104-stream round-3 REQUIRED FIX 1: a genuine SECOND result-type line must
// never overwrite the already-captured, already-durably-recorded FIRST result ----

#[test]
fn spawn_pins_the_returned_result_to_the_first_result_line_not_the_last() {
    // The rejected round's exact finding: `read_stream` kept consuming stdout lines
    // after capturing `result` (by design - a duplicate result is anticipated, not an
    // error), but unconditionally overwrote the in-memory `result` on every result-type
    // line, so the value `spawn()` RETURNED reflected the LAST result while
    // `record_result_if_absent` durably recorded only the FIRST - a returned-value vs
    // durable-store mismatch. This fixture emits the real recorded success stream (the
    // FIRST, genuine result), then a SECOND well-formed result carrying deliberately
    // different values in every field the reader maps, then a clean EOF - the opposite
    // shape from `spawn_survives_a_read_error_that_arrives_after_the_result_line`, which
    // errors after the result rather than emitting a second genuine one.
    let fx = Fixture::new();
    let bin = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/claude-code-second-result-agent.sh")
        .to_string_lossy()
        .into_owned();
    let driver = rigger::driver::claude_code::Driver { bin, ..fx.driver() };
    let o = opts("u104-stream/implementer#0");
    let emit = |_: &str, _: serde_json::Value| Ok(());

    let result = driver
        .spawn(&AgentDef::default(), "do the thing", &o, &emit)
        .expect("a second genuine result line must not fail the spawn");

    // The RETURNED AgentResult matches the FIRST result, never the second.
    assert_eq!(result.output, "done: the answer is 42");
    assert_eq!(result.resolved_model, "claude-sonnet-4-5-20250929");
    assert_ne!(
        result.output, "SECOND result - must never win",
        "the second result's own text must never win"
    );

    // Exactly one SpawnResult landed in the run store, and it is the FIRST result's own
    // full meta - never the second's.
    let events = fx
        .run_store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap();
    let results: Vec<_> = events
        .iter()
        .filter(|e| e.type_ == TYPE_SPAWN_RESULT)
        .collect();
    assert_eq!(results.len(), 1, "exactly one SpawnResult landed");
    let res = spawn::SpawnResult::from_event(results[0]).unwrap();
    assert_eq!(res.output, "done: the answer is 42");
    assert_eq!(
        res.meta["session_id"],
        "11111111-1111-4111-8111-111111111111"
    );
    assert_eq!(res.meta["usage"]["input"], 100);
    assert_eq!(res.meta["turns"], 3);
    assert_eq!(res.meta["cost_usd"], 0.0456);
}

// ---- adj-u104-stream round-5 REQUIRED FIX (op-104-stream-clean-eof-waits-before-the-reap):
// a clean EOF (the loop runs to real exhaustion, never the post-result read-error break)
// waits for the child to exit on its own BEFORE the reap ever considers a signal - round
// 4's consolidation onto `ReapedChild::drop`'s non-blocking `try_wait` force-ended a child
// still doing legitimate post-output teardown the instant its stdout pipe read EOF ----

#[test]
fn spawn_waits_out_a_clean_eof_before_reaping_never_kills_mid_teardown() {
    // The rejected round's exact finding, independently reproduced by sdet and the
    // adversary: a real agent can close stdout slightly before it finishes its own
    // teardown (flushing telemetry, releasing a lock). This fixture emits the real
    // recorded success stream (durably recording the FIRST result exactly like every
    // sibling fixture), closes its own stdout, THEN keeps running - a brief sleep
    // standing in for that legitimate post-output work - and only touches its own marker
    // once that work is done. A host that reaps on the bare EOF (the round-4 regression)
    // kills this process mid-sleep, so the marker is never written; the fix must instead
    // block until the child exits on its own, so `spawn()` returns the correct result
    // AND the marker is already there by the time it does.
    let fx = Fixture::new();
    let bin = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/claude-code-clean-eof-teardown-agent.sh")
        .to_string_lossy()
        .into_owned();
    let driver = rigger::driver::claude_code::Driver { bin, ..fx.driver() };
    let marker = fx.scratch_root.path().join("teardown-done.marker");
    let mut o = opts("u104-stream/implementer#0");
    o.env = vec![(
        "RIGGER_TEST_TEARDOWN_MARKER".to_string(),
        marker.to_string_lossy().into_owned(),
    )];
    let emit = |_: &str, _: serde_json::Value| Ok(());

    let result = driver
        .spawn(&AgentDef::default(), "do the thing", &o, &emit)
        .expect("a clean EOF followed by legitimate teardown must still return the result");

    assert_eq!(result.output, "done: the answer is 42");
    assert_eq!(result.resolved_model, "claude-sonnet-4-5-20250929");
    assert!(
        marker.exists(),
        "the child's own post-output teardown must finish before spawn() reaps it - a \
         child force-ended mid-teardown never gets to touch its marker"
    );
}

// ---- spec 104 criterion 6 (STOP): a wall-clock expiry against stream silence ----

#[test]
fn spawn_stops_gracefully_when_a_silent_child_winds_down_on_its_own() {
    // THE STOP (architecture addendum §4.6): "closes the session's input stream, waits a
    // grace period, then ends the child through the sanctioned lifecycle helper." This
    // fixture goes silent after `system/init` but exits ON ITS OWN the moment its stdin
    // closes - the well-behaved case, proving the grace period is honored rather than an
    // immediate force-end, while `stop_grace` is injected short so the test itself stays
    // fast (see `Driver::stop_grace`'s own doc).
    let fx = Fixture::new();
    let bin = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/claude-code-silent-winds-down-on-eof-agent.sh")
        .to_string_lossy()
        .into_owned();
    let driver = rigger::driver::claude_code::Driver {
        bin,
        stop_grace: Duration::from_millis(300),
        ..fx.driver()
    };
    let o = opts("u104-stop/implementer#0");
    let agent = AgentDef {
        max_wall_clock: Some(1),
        ..Default::default()
    };
    let emit = |_: &str, _: serde_json::Value| Ok(());

    let started = std::time::Instant::now();
    let err = driver
        .spawn(&agent, "do the thing", &o, &emit)
        .expect_err("a stream that never produces a result must not read as a success");
    let elapsed = started.elapsed();

    assert!(err.0.contains("stopped"), "{}", err.0);
    assert!(err.0.contains("u104-stop/implementer#0"), "{}", err.0);
    // 1s wall-clock bound + a short injected grace, nowhere near the real 30s default -
    // proves the injected `stop_grace` seam actually reached the stop sequence.
    assert!(
        elapsed < Duration::from_secs(5),
        "the stop must honor the INJECTED grace, not the real 30s default: {elapsed:?}"
    );

    // "the existing liveness-fault result is recorded" - the SAME SpawnResult shape
    // liveness::sweep records for the stepwise driver's own hung agent.
    let events = fx
        .run_store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap();
    let results: Vec<_> = events
        .iter()
        .filter(|e| e.type_ == TYPE_SPAWN_RESULT)
        .collect();
    assert_eq!(results.len(), 1, "exactly one liveness-fault result landed");
    let res = spawn::SpawnResult::from_event(results[0]).unwrap();
    assert_eq!(res.id, "u104-stop/implementer#0");
    assert!(
        res.is_liveness_fault(),
        "a wall-clock STOP must record the EXISTING liveness-fault shape, meta: {:?}",
        res.meta
    );
    assert_eq!(
        res.liveness_class(),
        rigger::failure::FailureClass::Infra.as_str()
    );

    // "the launch ends `stopped`" - the progress-store closing record.
    let progress_events = fx
        .progress_store
        .read_stream(rigger::progress::STREAM, 0, Direction::Forward)
        .unwrap();
    let launches: Vec<_> = progress_events
        .iter()
        .filter(|e| e.type_ == rigger::progress::TYPE_SPAWN_LAUNCHED)
        .collect();
    assert_eq!(
        launches.len(),
        2,
        "the open launch record plus its stopped closing record"
    );
    let closing: rigger::progress::SpawnLaunched =
        serde_json::from_slice(&launches[1].data).unwrap();
    assert_eq!(closing.ended.as_deref(), Some("stopped"));
    assert!(
        rigger::progress::open_launches(&progress_events)
            .unwrap()
            .is_empty(),
        "the launch must no longer read as open once STOP has closed it"
    );
}

#[test]
fn spawn_escalates_to_the_sanctioned_reap_when_a_silent_child_ignores_its_input_closing() {
    // The escalation half of THE STOP: a session that ignores its stdin closing entirely
    // (never exits on its own) must still be ended - through `reap::end_child`'s
    // handle-bound TERM-then-grace-then-KILL sequence, never a computed pid or a
    // shell-out - once the injected grace elapses. Proven the SAME way this file's own
    // mid-stream-read-error test proves a reap: a REAL pid the fixture wrote itself,
    // checked via `common::is_alive` independently of the `Child` handle that reaped it.
    let fx = Fixture::new();
    let bin = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/claude-code-silent-ignores-eof-agent.sh")
        .to_string_lossy()
        .into_owned();
    let driver = rigger::driver::claude_code::Driver {
        bin,
        stop_grace: Duration::from_millis(200),
        ..fx.driver()
    };
    let pid_file = fx.scratch_root.path().join("ignores-eof.pid");
    let mut o = opts("u104-stop/implementer#0");
    o.env = vec![(
        "RIGGER_TEST_IGNORES_EOF_PID_FILE".to_string(),
        pid_file.to_string_lossy().into_owned(),
    )];
    let agent = AgentDef {
        max_wall_clock: Some(1),
        ..Default::default()
    };
    let emit = |_: &str, _: serde_json::Value| Ok(());

    let err = driver
        .spawn(&agent, "do the thing", &o, &emit)
        .expect_err("a stream that never produces a result must not read as a success");
    assert!(err.0.contains("stopped"), "{}", err.0);

    let pid_text = std::fs::read_to_string(&pid_file)
        .expect("the fixture recorded its pid before going silent");
    let pid: u32 = pid_text
        .trim()
        .parse()
        .unwrap_or_else(|e| panic!("pid file {pid_text:?} did not parse: {e}"));
    assert!(
        !common::is_alive(pid),
        "child pid {pid} ignored its input closing and must still be ended by \
         reap::end_child's escalation - never leaked"
    );
}
