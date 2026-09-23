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

use std::path::Path;
use std::time::SystemTime;

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
