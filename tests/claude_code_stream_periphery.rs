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
//! `claude_code_launch_wire_periphery.rs`); relaunch and the composition-root swap's
//! BEHAVIOR under an actual failing session (spec 105 - the class exists here, but nothing
//! acts on it yet); the spawn-bound MCP server, the write guard and the StopFailure hooks'
//! own injection/command shape (criteria 3 and 4, separate units, already landed
//! independently - their tests, and `driver::claude_code`'s own in-file `mod tests` for
//! `classify_failure`, cover those). This
//! file's own FAILURE CLASS tests below (criterion 5) add only what only a real subprocess
//! can prove: the class a REAL child process's exit actually classifies as, stderr tail
//! included.

mod common;

use rigger::spawn::SpawnEvent;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use common::fixtures::{no_emit, CwdGuard};
use common::is_running;
use rigger::conductor::{AgentDriver, AgentFailure, SpawnOpts};
use rigger::config::AgentDef;
use rigger::driver::claude_code::Driver;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore};
use rigger::liveness;
use rigger::progress::{AgentProgress, TYPE_AGENT_PROGRESS};
use rigger::spawn::{self, TYPE_SPAWN_RESULT};

/// The fixture agent replaying the recorded success stream.
const STREAM_AGENT: &str = "claude-code-stream-agent.sh";

/// The absolute path of the fixture agent script `name` under `tests/fixtures`.
fn fixture_script(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
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

    fn driver(&self) -> Driver<'_> {
        Driver {
            bin: fixture_script(STREAM_AGENT),
            rigger_bin: "rigger".to_string(),
            progress_store: &self.progress_store,
            run_store: &self.run_store,
            scratch_root: self.scratch_root.path().to_string_lossy().into_owned(),
            stop_grace: std::time::Duration::from_secs(30),
        }
    }

    /// [`Fixture::driver`] launching the fixture agent script `script` instead.
    fn driver_running(&self, script: &str) -> Driver<'_> {
        Driver {
            bin: fixture_script(script),
            ..self.driver()
        }
    }

    /// [`Fixture::driver_running`] with THE STOP's grace injected as `stop_grace`.
    fn stopping_driver(&self, script: &str, stop_grace: Duration) -> Driver<'_> {
        Driver {
            stop_grace,
            ..self.driver_running(script)
        }
    }

    /// The file `name` directly under this fixture's scratch root.
    fn scratch_file(&self, name: &str) -> PathBuf {
        self.scratch_root.path().join(name)
    }

    /// Every `SpawnResult` the run store holds.
    fn spawn_results(&self) -> Vec<Event> {
        self.run_store
            .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
            .unwrap()
            .into_iter()
            .filter(|e| e.type_ == TYPE_SPAWN_RESULT)
            .collect()
    }

    /// Every `SpawnLaunched` record (open and closing alike) the progress store holds, in
    /// append order.
    fn launch_records(&self) -> Vec<rigger::progress::SpawnLaunched> {
        self.progress_store
            .read_stream(rigger::progress::STREAM, 0, Direction::Forward)
            .unwrap()
            .iter()
            .filter(|e| e.type_ == rigger::progress::TYPE_SPAWN_LAUNCHED)
            .map(|e| serde_json::from_slice(&e.data).unwrap())
            .collect()
    }

    /// Every agent-progress activity line the progress store holds.
    fn activities(&self) -> Vec<String> {
        self.progress_store
            .read_stream(rigger::progress::STREAM, 0, Direction::Forward)
            .unwrap()
            .iter()
            .filter(|e| e.type_ == TYPE_AGENT_PROGRESS)
            .map(|e| {
                serde_json::from_slice::<AgentProgress>(&e.data)
                    .unwrap()
                    .activity
            })
            .collect()
    }
}

fn opts(id: &str) -> SpawnOpts {
    SpawnOpts {
        id: id.to_string(),
        run_id: "run-1".to_string(),
        ..Default::default()
    }
}

/// [`opts`] whose spawn environment names `path` under the fixture's own variable `var`.
fn opts_with_env(id: &str, var: &str, path: &Path) -> SpawnOpts {
    SpawnOpts {
        env: vec![(var.to_string(), path.to_string_lossy().into_owned())],
        ..opts(id)
    }
}

/// An agent bounded to one second of wall clock, so a silent session is STOPped.
fn wall_clock_bounded() -> AgentDef {
    AgentDef {
        max_wall_clock: Some(1),
        ..Default::default()
    }
}

/// The pid a fixture agent wrote to `pid_file`; `recorded` names when it wrote it.
fn recorded_pid(pid_file: &Path, recorded: &str) -> u32 {
    let pid_text = std::fs::read_to_string(pid_file).expect(recorded);
    pid_text
        .trim()
        .parse()
        .unwrap_or_else(|e| panic!("pid file {pid_text:?} did not parse: {e}"))
}

/// Drive a wall-clock-bounded spawn that never produces a result, asserting it is
/// STOPped; returns the error and how long the STOP took.
fn stop(driver: &Driver, o: &SpawnOpts) -> (String, Duration) {
    let started = Instant::now();
    let err = driver
        .spawn(&wall_clock_bounded(), "do the thing", o, &no_emit)
        .expect_err("a stream that never produces a result must not read as a success");
    let elapsed = started.elapsed();
    assert!(err.0.contains("stopped"), "{}", err.0);
    (err.0, elapsed)
}

#[test]
fn spawn_reads_the_recorded_stream_to_a_real_result() {
    let fx = Fixture::new();
    let o = opts("u104-stream/implementer#0");

    let result = fx
        .driver()
        .spawn(&AgentDef::default(), "do the thing", &o, &no_emit)
        .expect("the recorded stream ends in a result");

    assert_eq!(result.output, "done: the answer is 42");
    assert_eq!(result.resolved_model, "claude-sonnet-4-5-20250929");
}

#[test]
fn spawn_records_the_result_in_the_run_store_with_its_full_meta() {
    let fx = Fixture::new();
    let o = opts("u104-stream/implementer#0");

    fx.driver()
        .spawn(&AgentDef::default(), "do the thing", &o, &no_emit)
        .unwrap();

    let results = fx.spawn_results();
    assert_eq!(results.len(), 1, "exactly one SpawnResult landed");
    let res = spawn::SpawnResult::from_event(&results[0]).unwrap();
    assert_eq!(res.id, "u104-stream/implementer#0");
    assert_eq!(res.output, "done: the answer is 42");
    assert!(!res.is_error());
    assert_eq!(
        res.meta_str(spawn::META_RESOLVED_MODEL),
        "claude-sonnet-4-5-20250929"
    );
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
    // One denial, both streamed as `system/permission_denied` and listed in the result's
    // own `permission_denials` array - counted once.
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

    fx.driver()
        .spawn(&AgentDef::default(), "first", &o, &no_emit)
        .unwrap();
    fx.driver()
        .spawn(&AgentDef::default(), "second", &o, &no_emit)
        .unwrap();

    assert_eq!(
        fx.spawn_results().len(),
        1,
        "the second spawn's result never clobbers the first"
    );
}

#[test]
fn spawn_touches_the_liveness_marker_and_persists_the_raw_stream() {
    let fx = Fixture::new();
    let o = opts("u104-stream/implementer#0");
    let scratch_root = fx.scratch_root.path().to_string_lossy().into_owned();

    fx.driver()
        .spawn(&AgentDef::default(), "do the thing", &o, &no_emit)
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

/// The recorded stream's activity lines, as a real spawn turns them into progress reports,
/// carry a line containing every one of each `needles` group.
fn spawn_reports_progress(needles: &[&[&str]]) {
    let fx = Fixture::new();
    let o = opts("u104-stream/implementer#0");

    fx.driver()
        .spawn(&AgentDef::default(), "do the thing", &o, &no_emit)
        .unwrap();

    let activities = fx.activities();
    for group in needles {
        assert!(
            activities
                .iter()
                .any(|a| group.iter().all(|needle| a.contains(needle))),
            "activities: {activities:?}"
        );
    }
}

rigger::test_cases! {
    spawn_turns_api_retry_and_the_unparseable_line_into_progress_reports: spawn_reports_progress(&[
        &["rate_limit", "618"],
        &["permission denied"],
        &["unparseable", "not json at all"],
    ]);
    // `system/init`'s `mcp_servers` array is "noted for the record" (Design) - this
    // file's other progress-line assertions cover api_retry/denied/unparseable but
    // never this branch of the same match arm; close it.
    spawn_records_the_mcp_connection_status_from_system_init_as_a_progress_line:
        spawn_reports_progress(&[&["mcp servers", "rigger", "connected"]]);
}

/// A spawn through `driver` fails with an error naming the spawn (`failing` says why it must
/// not succeed) and records no `SpawnResult`.
fn spawn_fails_recording_nothing(fx: &Fixture, driver: &Driver, failing: &str) {
    let o = opts("u104-stream/implementer#0");

    let err = driver
        .spawn(&AgentDef::default(), "do the thing", &o, &no_emit)
        .expect_err(failing);
    assert!(err.0.contains("u104-stream/implementer#0"), "{}", err.0);
    assert!(fx.spawn_results().is_empty());
}

#[test]
fn spawn_errors_loudly_when_the_stream_ends_with_no_result() {
    // The reader's OWN correctness, distinct from failure CLASSIFICATION (criterion 5,
    // not this one's): a stream that never produces a `result` must never read as a
    // silent success - and no SpawnResult is recorded either.
    let fx = Fixture::new();
    spawn_fails_recording_nothing(
        &fx,
        &fx.driver_running("claude-code-stream-no-result-agent.sh"),
        "a stream with no result must not read as success",
    );
}

// ---- FAILURE CLASS (spec 104 criterion 5): real-subprocess classification ----

/// The failure a spawn of `script` ends in, after a StopFailure record of `billing_error` is
/// pre-seeded on run `seeded_run` (when given), asserting its class is `class` (`why` says
/// why it must be); returns the error text.
fn spawn_fails_classified(
    script: &str,
    seeded_run: Option<&str>,
    class: AgentFailure,
    why: &str,
) -> String {
    let fx = Fixture::new();
    if let Some(run) = seeded_run {
        rigger::progress_store::record_stop_failure(
            &fx.progress_store,
            run,
            &rigger::progress::StopFailure {
                spawn: "u104-fail-class/implementer#0".to_string(),
                class: "billing_error".to_string(),
            },
        )
        .unwrap();
    }
    let o = opts("u104-fail-class/implementer#0"); // run_id: "run-1"

    let err = fx
        .driver_running(script)
        .spawn(&AgentDef::default(), "do the thing", &o, &no_emit)
        .expect_err("a session ending with no result must not succeed");

    assert!(
        rigger::conductor::strip_failure_marker(&err).contains(&format!("class {class}")),
        "{why}: {}",
        err.0
    );
    assert_closed_as_a_fault(&fx, class);
    err.0
}

/// The fixture's latest launch record closes the launch `fault` with `class` (spec 104 THE
/// LAUNCH: "at exit it closes the record with `ended: completed | interrupted | fault |
/// stopped` and the class").
fn assert_closed_as_a_fault(fx: &Fixture, class: AgentFailure) {
    let closing = fx.launch_records().pop().expect("the launch was recorded");
    assert_eq!(closing.ended.as_deref(), Some("fault"), "{closing:?}");
    assert_eq!(
        closing.class.as_deref(),
        Some(class.as_str()),
        "{closing:?}"
    );
}

#[test]
fn a_child_that_exits_before_init_classifies_unknown_and_carries_the_stderr_tail() {
    // CONSTRAINTS WALK, verbatim: "the child exits before `system/init` (binary missing,
    // unknown flag) - a fault of class `unknown` carrying the stderr tail, never a hang."
    // No StopFailure record, no api_retry line - the `unknown` floor is all that is left.
    let err = spawn_fails_classified(
        "claude-code-exits-before-init-agent.sh",
        None,
        AgentFailure::Unknown,
        "a child that never reaches system/init classifies unknown",
    );
    assert!(
        err.contains("unrecognized flag --this-flag-does-not-exist"),
        "the stderr tail must be visible in the error: {err}"
    );
}

rigger::test_cases! {
    a_session_with_no_stopfailure_record_classifies_from_the_last_api_retry_category:
        spawn_fails_classified(
            "claude-code-api-retry-then-no-result-agent.sh",
            None,
            AgentFailure::AuthenticationFailed,
            "a session ending on an api_retry with no result classifies from that category",
        );
    // Design's FAILURE CLASS ordering: "the record written by the `StopFailure` hook ...
    // else the last `api_retry.error`". Pre-seed a StopFailure record for a DIFFERENT
    // category than the fixture's own api_retry line, as `rigger hook stop-failure` would
    // have written it, and prove the record wins.
    a_stopfailure_record_outranks_the_last_api_retry_category: spawn_fails_classified(
        "claude-code-api-retry-then-no-result-agent.sh",
        Some("run-1"),
        AgentFailure::BillingError,
        "the StopFailure record must outrank the api_retry category",
    );
    // adv-u104c5-stopfailure-crosses-run-boundary: a StopFailure record left over from an
    // OLD or UNRELATED run must never outrank the LIVE session's own api_retry category -
    // the same `run_id` scoping `rigger status` already applies to this exact progress
    // stream (`src/main.rs`). The reciprocal of the sibling case just above: same spawn id,
    // same seeded class, but stamped on a DIFFERENT run than this session's own "run-1".
    a_stopfailure_record_from_a_different_run_does_not_outrank_the_live_sessions_api_retry:
        spawn_fails_classified(
            "claude-code-api-retry-then-no-result-agent.sh",
            Some("some-other-run"),
            AgentFailure::AuthenticationFailed,
            "the OTHER run's StopFailure record must not outrank this run's own api_retry \
             category",
        );
}

// ---- LAUNCH FAULT (spec 104 THE STREAM): "a `rigger` server that did not connect fails
// the launch as a fault" ----

#[cfg(unix)]
#[test]
fn a_rigger_mcp_server_that_failed_at_init_stops_the_session_as_an_unknown_fault() {
    // 2026-09-28 probe against the real claude 2.1.283: the spawn's `rigger mcp --spawn`
    // server exited before it connected, init reported it `failed`, and the session ran its
    // whole task with no rigger tools while the host recorded a success. The fixture replays
    // that init, then carries on toward a result the host must never read.
    let fx = Fixture::new();
    let pid_file = fx.scratch_file("agent.pid");
    let o = opts_with_env(
        "u104-mcp/implementer#0",
        "RIGGER_TEST_MCP_FAILED_PID_FILE",
        &pid_file,
    );

    let started = Instant::now();
    let err = fx
        .driver_running("claude-code-mcp-server-failed-agent.sh")
        .spawn(&AgentDef::default(), "do the thing", &o, &no_emit)
        .expect_err("a session whose rigger MCP server failed must not read as a success");
    let elapsed = started.elapsed();

    let message = rigger::conductor::strip_failure_marker(&err);
    assert!(
        message.contains(&format!("class {}", AgentFailure::Unknown)),
        "a launch fault is class unknown: {}",
        err.0
    );
    assert!(
        message.contains("u104-mcp/implementer#0")
            && message.contains("rigger MCP server")
            && message.contains("\"failed\""),
        "the failure names the spawn, the server and its reported status: {message}"
    );
    // Stopped at the init line, never after the fixture's toolless work: the session is
    // starting its task, not winding down, so no grace is spent on it.
    assert!(
        elapsed < Duration::from_secs(3),
        "the session must be stopped at init, not left to run: {elapsed:?}"
    );
    let pid = recorded_pid(&pid_file, "the fixture recorded its pid before init");
    assert!(
        !common::is_alive(pid),
        "child pid {pid} must not outlive its launch fault"
    );
    assert!(
        fx.spawn_results().is_empty(),
        "a launch fault records no result of its own"
    );
    assert_closed_as_a_fault(&fx, AgentFailure::Unknown);
}

#[test]
fn a_connected_rigger_mcp_server_proceeds_whatever_another_server_reports() {
    let fx = Fixture::new();
    let o = opts("u104-mcp/implementer#0");

    let result = fx
        .driver_running("claude-code-mcp-server-connected-agent.sh")
        .spawn(&AgentDef::default(), "do the thing", &o, &no_emit)
        .expect("a connected rigger server is a healthy launch");

    assert_eq!(result.output, "done with the rigger tools");
    assert_eq!(fx.spawn_results().len(), 1);
}

// ---- THE STREAM's denial count: the `result` event's `permission_denials` is its one source ----

/// The `permission_denials` count the run store holds after a spawn of `script`.
fn recorded_denials(script: &str) -> serde_json::Value {
    let fx = Fixture::new();
    let o = opts("u104-denials/implementer#0");

    fx.driver_running(script)
        .spawn(&AgentDef::default(), "do the thing", &o, &no_emit)
        .expect("the fixture stream ends in a result");

    let results = fx.spawn_results();
    assert_eq!(results.len(), 1, "exactly one SpawnResult landed");
    spawn::SpawnResult::from_event(&results[0]).unwrap().meta["permission_denials"].clone()
}

rigger::test_cases! {
    // 2026-09-28 probe: a PreToolUse hook block emits no `system/permission_denied` event,
    // yet the result lists it.
    a_hook_blocked_denial_with_no_stream_event_is_counted_from_the_result:
        assert_eq!(recorded_denials("claude-code-hook-blocked-denial-agent.sh"), 1);
    a_denial_both_streamed_and_listed_in_the_result_is_counted_once:
        assert_eq!(recorded_denials(STREAM_AGENT), 1);
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
    // SAME failure surfaces through the composed call, not just the half-call. No
    // SpawnResult lands either - a launch that never started a process never produced a
    // stream to read a result from.
    let fx = Fixture::new();
    let driver = Driver {
        bin: "/definitely/does/not/exist/claude-code-xyz".to_string(),
        ..fx.driver()
    };
    spawn_fails_recording_nothing(
        &fx,
        &driver,
        "spawning a nonexistent binary must fail through spawn(), not just launch()",
    );
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
    let driver = Driver {
        scratch_root: String::new(),
        ..fx.driver()
    };
    let o = opts("u104-stream/implementer#0");

    driver
        .spawn(&AgentDef::default(), "do the thing", &o, &no_emit)
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
    let pid_file = fx.scratch_file("agent.pid");
    let o = opts_with_env(
        "u104-stream/implementer#0",
        "RIGGER_TEST_INVALID_UTF8_PID_FILE",
        &pid_file,
    );

    let err = fx
        .driver_running("claude-code-invalid-utf8-agent.sh")
        .spawn(&AgentDef::default(), "do the thing", &o, &no_emit)
        .expect_err("invalid utf-8 on the stream must surface as a read error, not a result");
    assert!(err.0.contains("read agent stream"), "{}", err.0);

    let pid = recorded_pid(
        &pid_file,
        "the fixture recorded its pid before writing invalid utf-8",
    );
    assert!(
        !common::is_alive(pid),
        "child pid {pid} must not survive a mid-stream read error - it must be reaped \
         (through dash::ReapedChild), never leaked"
    );

    // No SpawnResult landed either - a stream that errored mid-read never reached a
    // `result` message.
    assert!(fx.spawn_results().is_empty());
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
    let (tx, rx) = std::sync::mpsc::channel::<Result<String, String>>();
    std::thread::spawn(move || {
        let fx = Fixture::new();
        let o = opts("u104-stream/implementer#0");
        let outcome = fx
            .driver_running("claude-code-stderr-flood-agent.sh")
            .spawn(&AgentDef::default(), "do the thing", &o, &no_emit)
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
    let pid_file = fx.scratch_file("agent.pid");
    let o = opts_with_env(
        "u104-stream/implementer#0",
        "RIGGER_TEST_POST_RESULT_PID_FILE",
        &pid_file,
    );

    let result = fx
        .driver_running("claude-code-post-result-read-error-agent.sh")
        .spawn(&AgentDef::default(), "do the thing", &o, &no_emit)
        .expect("a read error strictly after the result must not overturn it");
    assert_eq!(result.output, "done: the answer is 42");
    assert_eq!(result.resolved_model, "claude-sonnet-4-5-20250929");

    // Exactly one SpawnResult landed - the real one, durably recorded before the
    // later read error ever happened.
    assert_eq!(
        fx.spawn_results().len(),
        1,
        "exactly one SpawnResult landed"
    );

    // The child is still reaped through its own handle, never leaked, even though a
    // read error - not a clean EOF - is what ended this loop.
    let pid = recorded_pid(
        &pid_file,
        "the fixture recorded its pid before writing invalid utf-8",
    );
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
    let o = opts("u104-stream/implementer#0");

    let result = fx
        .driver_running("claude-code-second-result-agent.sh")
        .spawn(&AgentDef::default(), "do the thing", &o, &no_emit)
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
    let results = fx.spawn_results();
    assert_eq!(results.len(), 1, "exactly one SpawnResult landed");
    let res = spawn::SpawnResult::from_event(&results[0]).unwrap();
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
    let marker = fx.scratch_file("teardown-done.marker");
    let o = opts_with_env(
        "u104-stream/implementer#0",
        "RIGGER_TEST_TEARDOWN_MARKER",
        &marker,
    );

    let result = fx
        .driver_running("claude-code-clean-eof-teardown-agent.sh")
        .spawn(&AgentDef::default(), "do the thing", &o, &no_emit)
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
    let driver = fx.stopping_driver(
        "claude-code-silent-winds-down-on-eof-agent.sh",
        Duration::from_millis(300),
    );
    let o = opts("u104-stop/implementer#0");

    let (err, elapsed) = stop(&driver, &o);

    assert!(err.contains("u104-stop/implementer#0"), "{err}");
    // 1s wall-clock bound + a short injected grace, nowhere near the real 30s default -
    // proves the injected `stop_grace` seam actually reached the stop sequence.
    assert!(
        elapsed < Duration::from_secs(5),
        "the stop must honor the INJECTED grace, not the real 30s default: {elapsed:?}"
    );

    // "the existing liveness-fault result is recorded" - the SAME SpawnResult shape
    // liveness::sweep records for the stepwise driver's own hung agent.
    let results = fx.spawn_results();
    assert_eq!(results.len(), 1, "exactly one liveness-fault result landed");
    let res = spawn::SpawnResult::from_event(&results[0]).unwrap();
    assert_eq!(res.id, "u104-stop/implementer#0");
    assert!(
        res.is_liveness_fault(),
        "a wall-clock STOP must record the EXISTING liveness-fault shape, meta: {:?}",
        res.meta
    );
    assert_eq!(
        res.meta_str(spawn::META_LIVENESS_CLASS),
        rigger::failure::FailureClass::Infra.as_str()
    );

    // "the launch ends `stopped`" - the progress-store closing record.
    let launches = fx.launch_records();
    assert_eq!(
        launches.len(),
        2,
        "the open launch record plus its stopped closing record"
    );
    assert_eq!(launches[1].ended.as_deref(), Some("stopped"));
}

#[test]
fn spawn_stop_grace_loop_actually_waits_out_the_injected_duration() {
    // Pins the grace-period LOOP itself, not just its outer bound: a host whose deadline
    // is computed BEHIND `now()` (never ahead of it), or whose `while now() < deadline`
    // bound check never iterates at all, reaches `reap::end_child`'s SIGTERM the instant
    // the child's input closes - before this fixture's own deliberate post-EOF sleep
    // completes - so the fixture is killed mid-sleep and never writes its marker. Only a
    // host that genuinely polls across the injected `stop_grace` window observes the
    // fixture exiting ON ITS OWN and lets it finish, so the marker survives.
    let fx = Fixture::new();
    // Comfortably longer than the fixture's own 0.15s post-EOF sleep, so a correct
    // host's poll loop is certain to observe the natural exit within the grace
    // window rather than racing it.
    let driver = fx.stopping_driver(
        "claude-code-silent-delayed-self-exit-agent.sh",
        Duration::from_millis(600),
    );
    let marker = fx.scratch_file("self-exit.marker");
    let o = opts_with_env(
        "u104-stop/implementer#1",
        "RIGGER_TEST_SELF_EXIT_MARKER",
        &marker,
    );

    stop(&driver, &o);

    assert!(
        marker.exists(),
        "the grace loop must actually WAIT for the fixture's own delayed self-exit \
         (proving the deadline is computed ahead of `now()` and the loop's bound check \
         truly iterates) rather than force-ending it before its post-EOF sleep completes"
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
    let driver = fx.stopping_driver(
        "claude-code-silent-ignores-eof-agent.sh",
        Duration::from_millis(200),
    );
    let pid_file = fx.scratch_file("ignores-eof.pid");
    let o = opts_with_env(
        "u104-stop/implementer#0",
        "RIGGER_TEST_IGNORES_EOF_PID_FILE",
        &pid_file,
    );

    stop(&driver, &o);

    let pid = recorded_pid(
        &pid_file,
        "the fixture recorded its pid before going silent",
    );
    assert!(
        !common::is_alive(pid),
        "child pid {pid} ignored its input closing and must still be ended by \
         reap::end_child's escalation - never leaked"
    );
}

#[test]
fn a_concurrent_sibling_spawns_process_in_the_same_worktree_survives_a_wall_clock_stop() {
    // The gap `adv-u104stop-r2-stop-reaps-concurrent-sibling-worktree` named: a worktree-wide
    // sweep at wall-clock STOP would signal every process rooted in the spawn's worktree
    // dir, but that dir is shared, concurrently, by every lens of the same unit's review
    // fan-out (`run_review_agents_concurrently`, up to `MAX_CONCURRENCY`), and
    // `reap::is_signal_eligible` matches purely on cwd containment - it has no notion of
    // which spawn owns a process. So THE STOP must never sweep the worktree at all
    // (`op-104-stop-no-sweep-at-wall-clock-stop`): it ends only the one child handle it
    // holds. Proven here the SAME way the removed sweep-proving test proved the opposite
    // case: a REAL process standing in for a live SIBLING spawn, rooted in this spawn's own
    // worktree dir via the SAME public `UNIT_WORKTREE_PREFIX` constant production derives it
    // from, that must still be running once this spawn's own wall-clock stop returns.
    let fx = Fixture::new();
    let driver = fx.stopping_driver(
        "claude-code-silent-ignores-eof-agent.sh",
        Duration::from_millis(200),
    );
    let scratch_root = fx.scratch_root.path().to_string_lossy().into_owned();
    let unit_dir = format!(
        "{scratch_root}/{}u104-stop",
        rigger::worktree::UNIT_WORKTREE_PREFIX
    );
    std::fs::create_dir_all(&unit_dir).unwrap();
    // Stands in for a concurrently alive SIBLING spawn's own process (another review
    // lens's live cargo build/test) rooted in the SAME unit worktree - never the process
    // this test's own spawn below will be stopped through.
    let mut sibling = std::process::Command::new("sleep")
        .arg("300")
        .current_dir(&unit_dir)
        .spawn()
        .expect("spawn sleep");
    let sibling_pid = sibling.id();

    let pid_file = fx.scratch_file("ignores-eof-sibling.pid");
    let o = opts_with_env(
        "u104-stop/implementer#0",
        "RIGGER_TEST_IGNORES_EOF_PID_FILE",
        &pid_file,
    );

    stop(&driver, &o);

    // The stopped spawn's OWN held child is still ended - exactly as
    // `spawn_escalates_to_the_sanctioned_reap_when_a_silent_child_ignores_its_input_closing`
    // already proves - so this test's new assertion below is what THE STOP must leave
    // alone, not a claim that STOP now reaps nothing at all.
    let stopped_pid = recorded_pid(
        &pid_file,
        "the fixture recorded its pid before going silent",
    );
    assert!(
        !common::is_alive(stopped_pid),
        "the stopped spawn's own held child must still be ended by reap::end_child"
    );

    // The SIBLING - sharing the same worktree cwd, never held by this spawn's own `Child`
    // handle - must be untouched: `try_wait` reads `Ok(None)` (still running; a reaped
    // process reads `Ok(Some(_))`), and `is_alive` confirms it a second, independent way.
    assert_eq!(
        sibling.try_wait().unwrap(),
        None,
        "pid {sibling_pid}, standing in for a live SIBLING spawn's own process in the same \
         worktree, must survive another spawn's wall-clock stop - THE STOP ends only the \
         one child handle it holds, never a worktree-wide sweep"
    );
    assert!(
        common::is_alive(sibling_pid),
        "pid {sibling_pid} must still be alive - a worktree sweep at wall-clock STOP would \
         have signalled a live sibling spawn's own process"
    );

    // This test is the sibling's real parent - end it through the SAME sanctioned
    // handle-bound path (`Child::kill` + `Child::wait`) rather than leaking it.
    sibling.kill().expect("end the sibling stand-in process");
    let _ = sibling.wait();
}

// ---- spec 104 criterion 6 round-4 fix (decision op-104-stop-end-the-tree-and-bound-the-
// joins): a descendant the driven child forked but never exec'd, inheriting a pipe fd ----

#[test]
fn spawn_stop_ends_a_forked_descendant_still_in_the_childs_own_process_tree() {
    // adj-u104stop-r3-verdict-reject UPHELD adv-u104stop-r3-stop-can-still-hang-on-a-
    // descendant: a descendant the driven child forked (`sleep 60 &`, never `exec`'d, so it
    // stays a genuine CHILD of the fixture's own shell) inherits the stdout pipe's write
    // end before the shell goes silent. reap::end_child's new PID-TREE walk
    // (descendants_of, snapshotted before the child's own first signal) must find this
    // descendant and end it too - never left running merely because THE STOP only ever
    // held a `Child` handle to its direct parent - and THE STOP must still return promptly.
    let fx = Fixture::new();
    let driver = fx.stopping_driver(
        "claude-code-descendant-in-tree-agent.sh",
        Duration::from_millis(300),
    );
    let descendant_pid_file = fx.scratch_file("descendant-in-tree.pid");
    let o = opts_with_env(
        "u104-stop/implementer#0",
        "RIGGER_TEST_DESCENDANT_PID_FILE",
        &descendant_pid_file,
    );

    let (_, elapsed) = stop(&driver, &o);

    assert!(
        elapsed < Duration::from_secs(5),
        "THE STOP must return within a bounded time even while ending a forked \
         descendant's own process tree: {elapsed:?}"
    );

    let descendant_pid = recorded_pid(
        &descendant_pid_file,
        "the fixture recorded its forked descendant's pid before going silent",
    );
    assert!(
        common::wait_until(|| !is_running(descendant_pid)),
        "a descendant still in the child's own process tree (pid {descendant_pid}) must be \
         ended by reap::end_child's new pid-tree walk, not merely left to outlive THE STOP"
    );
}

#[test]
fn spawn_stop_returns_within_bound_when_a_descendant_has_already_escaped_the_childs_tree() {
    // The bounded-join backstop half of the SAME round-4 fix: a descendant that had
    // ALREADY double-forked itself out of the driven child's own process tree before THE
    // STOP's pid-tree snapshot ever ran (reparented to init/a subreaper, so
    // reap::end_child's walk can never find it) still holds a duplicate of the stdout
    // pipe's write end open. THE STOP must still return within a bounded time regardless -
    // Driver::join_within (bounded by self.stop_grace on this path), not reap::end_child,
    // is what closes this gap - and must never touch this pid, since by the time the walk
    // runs it is no longer any part of the child's own tree at all.
    //
    // stop_grace is injected here as 4s - deliberately LARGER than the ordinary path's own
    // dedicated ORDINARY_DRAIN_JOIN_BOUND (2s, round-5 fix,
    // op-104-stop-ordinary-path-drain-bound) - rather than the smaller value a "just prove
    // it returns" test would use: the escaped descendant holds BOTH the stdout and stderr
    // pipe copies open (like the ordinary-path fixture below), so the two sequential
    // trailing joins deterministically block for the FULL injected `stop_grace` each,
    // never racing a real EOF. That makes the total elapsed a direct, load-bearing readout
    // of WHICH constant `Driver::join_within` actually used on this path: at this 4s
    // value, correctly using `self.stop_grace` measures ~9s (baseline wall-clock trigger
    // plus 2x4s, confirmed empirically); a regression that silently reused the smaller
    // `ORDINARY_DRAIN_JOIN_BOUND` here instead - exactly the constant the round-5 fix
    // introduced one call site away - would measure ~5s instead, indistinguishable from
    // "prompt" under the round-4 test's own loose `elapsed < 5s` ceiling alone. Both a
    // floor and a ceiling below turn that gap into a fast, deterministic failure rather
    // than a silent pass.
    let fx = Fixture::new();
    let driver = fx.stopping_driver(
        "claude-code-descendant-out-of-tree-agent.sh",
        Duration::from_secs(4),
    );
    let descendant_pid_file = fx.scratch_file("descendant-out-of-tree.pid");
    let o = opts_with_env(
        "u104-stop/implementer#0",
        "RIGGER_TEST_DESCENDANT_PID_FILE",
        &descendant_pid_file,
    );

    let (_, elapsed) = stop(&driver, &o);

    assert!(
        elapsed > Duration::from_secs(7),
        "THE STOP's trailing pipe joins must be bounded by the INJECTED self.stop_grace \
         (4s here), never silently downgraded to the smaller, un-injected \
         ORDINARY_DRAIN_JOIN_BOUND the ordinary (non-STOP) path uses - an elapsed time this \
         low means the wrong constant governed this path: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(20),
        "THE STOP must still return within a bounded time even when a descendant that \
         already escaped the child's own process tree still holds the stdout pipe's write \
         end open: {elapsed:?}"
    );

    let descendant_pid = recorded_pid(
        &descendant_pid_file,
        "the fixture recorded its escaped descendant's pid before going silent",
    );
    assert!(
        common::is_alive(descendant_pid),
        "pid {descendant_pid}, standing in for a descendant that already escaped the \
         child's own process tree, must be left untouched by THE STOP - it is not part of \
         the tree reap::end_child's pid-tree walk can ever find, only a stray holder of \
         its pipe"
    );

    // This test's own cleanup, through the sanctioned test-side signal call - never any
    // process rigger's own STOP is responsible for reaping.
    common::terminate_pid(descendant_pid);
}

#[test]
fn spawn_returns_a_real_result_promptly_even_when_a_descendant_still_holds_the_stdout_pipe() {
    // The round-4 bounded-join fix (op-104-stop-end-the-tree-and-bound-the-joins) applies
    // Driver::join_within at FOUR call sites, not just the two inside
    // stop_for_wall_clock_silence the two tests above already prove: read_stream's own
    // ORDINARY exit - a genuine result lands, no wall-clock silence, THE STOP never runs
    // at all - hits the identical two joins right after dash::ReapedChild::drop, which
    // ends only the ONE held child and (unlike reap::end_child) never walks a process
    // tree. Before the round-4 fix this path's joins were bare `join()` calls, so a
    // descendant inheriting the stdout pipe would have hung spawn() forever on a
    // perfectly ordinary, successful run - a boundary bug with zero periphery coverage on
    // this non-STOP path until now. This proves the ordinary-completion half: a real
    // result is still captured and returned, within a bounded time, and the descendant
    // itself is left running untouched (this path never attempts to end it - only THE
    // STOP's own reap::end_child does that, by design).
    //
    // adj-u104stop-r4-verdict-reject REQUIRED FIX (op-104-stop-ordinary-path-drain-bound):
    // this path's two joins are bounded by a dedicated constant
    // (`claude_code::ORDINARY_DRAIN_JOIN_BOUND`, a few seconds), never by `stop_grace` -
    // `stop_grace` is THE STOP's own wait-for-a-graceful-exit concern and has no meaning
    // here, where the child has already exited cleanly. Proving "promptly" at a shrunk
    // `stop_grace` (the prior version of this test overrode it to 300ms) would prove
    // nothing about the real bound this path now uses, so this `Driver` takes the
    // PRODUCTION `stop_grace` default (`fx.driver()`'s own 30s) unmodified - only the
    // STOP-path tests above still shrink `stop_grace`, for THEIR own concern.
    let fx = Fixture::new();
    let driver = fx.driver_running("claude-code-descendant-survives-a-clean-result-agent.sh");
    let descendant_pid_file = fx.scratch_file("descendant-clean-result.pid");
    let o = opts_with_env(
        "u104-stop/implementer#0",
        "RIGGER_TEST_DESCENDANT_PID_FILE",
        &descendant_pid_file,
    );

    let started = Instant::now();
    let result = driver
        .spawn(&wall_clock_bounded(), "do the thing", &o, &no_emit)
        .expect(
            "a real result line must still be read back even though a descendant \
             outlives the driven child and keeps the stdout pipe's write end open",
        );
    let elapsed = started.elapsed();

    assert_eq!(
        result.output,
        "done: a result survives a still-open descendant pipe"
    );
    // The fixture's forked descendant inherits copies of BOTH the stdout and stderr
    // write ends, so both joins (stderr drain, then stdout reader, run sequentially)
    // independently hit `claude_code::ORDINARY_DRAIN_JOIN_BOUND` before detaching -
    // worst case is ~2x that bound, plus slack for process/scheduling overhead. This is
    // what proves "promptly" at the real production `stop_grace` (unmodified above):
    // the ordinary path no longer owes that field anything.
    assert!(
        elapsed < Duration::from_secs(6),
        "an ordinary completion must return within the dedicated ordinary-path drain \
         bound (never stop_grace) even when a descendant still holds the stdout pipe \
         open: {elapsed:?}"
    );

    let descendant_pid = recorded_pid(
        &descendant_pid_file,
        "the fixture recorded its forked descendant's pid before exiting",
    );
    assert!(
        common::is_alive(descendant_pid),
        "pid {descendant_pid}: an ordinary completion's reap ends only the ONE held \
         child (dash::ReapedChild::drop), never a descendant - this is the join bound's \
         job, not a sweep, so the descendant must be left running"
    );

    // This test's own cleanup, through the sanctioned test-side signal call - never any
    // process rigger's own ordinary completion path is responsible for reaping.
    common::terminate_pid(descendant_pid);
}

// ---- THE STOP's bound: `max_wall_clock` 0 is unbounded, never a 5 s stop ----

/// The silent-past-the-poll fixture (silent 7 s, then a result), launched with no wall
/// clock bound.
const SILENT_PAST_THE_POLL: &str = "claude-code-silent-past-the-poll-then-result-agent.sh";

#[test]
fn an_unbounded_launch_silent_past_the_poll_still_records_its_result() {
    // 2026-09-28: the unbounded launch's 5 s wake-up took the wall-clock STOP branch, so a
    // session that thought in silence for 5 s was stopped as a liveness fault reading
    // "silent for 0s" - every real agent's long first turn.
    let fx = Fixture::new();
    let o = opts("u104-unbounded/implementer#0");

    let result = fx
        .driver_running(SILENT_PAST_THE_POLL)
        .spawn(&AgentDef::default(), "do the thing", &o, &no_emit)
        .expect("an unbounded launch waits for its result however long the silence");

    assert_eq!(result.output, "done: thought past the poll");
    let results = fx.spawn_results();
    assert_eq!(results.len(), 1, "exactly one SpawnResult landed");
    let res = spawn::SpawnResult::from_event(&results[0]).unwrap();
    assert!(
        !res.is_error() && !res.is_liveness_fault(),
        "no fault fires on an unbounded launch: {res:?}"
    );
}

#[test]
fn a_bounded_launch_silent_past_its_wall_clock_is_still_stopped() {
    let fx = Fixture::new();
    let o = opts("u104-unbounded/implementer#0");

    let (err, _elapsed) = stop(
        &fx.stopping_driver(SILENT_PAST_THE_POLL, Duration::from_millis(200)),
        &o,
    );

    assert!(err.contains("silent for 1s"), "{err}");
    let results = fx.spawn_results();
    assert_eq!(results.len(), 1, "exactly one liveness-fault result landed");
    assert!(spawn::SpawnResult::from_event(&results[0])
        .unwrap()
        .is_liveness_fault());
}

#[test]
fn an_unbounded_launch_whose_child_exited_behind_a_held_pipe_ends_with_no_result() {
    // No EOF ever arrives (a descendant still holds the stdout pipe), so on an unbounded
    // launch the wake-up re-checking the child is the only thing that sees the session end.
    let started = Instant::now();
    let err = spawn_fails_classified(
        "claude-code-exits-leaving-a-descendant-on-the-pipe-agent.sh",
        None,
        AgentFailure::Unknown,
        "a session that exited with no result classifies unknown",
    );
    assert!(err.contains("ended with no result"), "{err}");
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "the read ends at the wake-up after the child exits: {:?}",
        started.elapsed()
    );
}
