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
//! `claude_code_launch_wire_periphery.rs`); the composition-root swap's BEHAVIOR under an
//! actual failing session (spec 105); the spawn-bound MCP server, the write guard and the StopFailure hooks'
//! own injection/command shape (criteria 3 and 4, separate units, already landed
//! independently - their tests, and `driver::claude_code`'s own in-file `mod tests` for
//! `classify_failure`, cover those). This
//! file's own FAILURE CLASS tests below (criterion 5) add only what only a real subprocess
//! can prove: the class a REAL child process's exit actually classifies as, stderr tail
//! included.
//!
//! The LIVENESS tests at the end (spec 105) add what only a real child can prove of the
//! `ApiRetry` record: it lands in the run's progress stream beside its waiting line, it is on
//! record while the session still waits the delay out, a refused append costs the session
//! nothing, and a session ending with no result keeps it.
//!
//! THE SILENT-TIME tests ahead of them (spec 105) add what only a real child can prove of the
//! stream-silence stop: it fires on silent time, not on the time since the last line, over the
//! real records the host reads; a host with no beat source counts from the last line.
//!
//! The HOLD tests after them (spec 105) drive the disposition table's transitions through a real
//! child whose whole wire a table row states: one `RunHeld` per transition and none on a repeat or
//! an operator hold, a launch's close and the revision it read, an `is_error` result classed after
//! the child exits, an earlier launch's `StopFailure`, and a third consecutive failed close.
//!
//! THE RESUME FALLBACK tests after them (spec 105) add what only a real child can prove of a
//! `--resume` launch: the ways it can yield no `system/init`, grounded on the literal refusal of
//! Claude Code 2.1.314, each close it `interrupted` and land one fresh launch at the next ordinal
//! inside the same `spawn` call; a launch that saw `system/init`, and a launch that was fresh,
//! stand alone; and every launch takes the ordinal after the highest the progress store records.
//!
//! THE RELAUNCH table at the end (spec 105) adds what only a real child can prove of a spawn whose
//! latest launch ended `interrupted`: the host `--resume`s that launch's session at the next
//! ordinal in the same directory, its first message the note its close decides (the hold a release
//! ended, the supervisor's restart, or the class alone), then the `resume_task` that launch
//! carried; any other latest launch, or one of another run, launches fresh; a relaunch with no
//! `system/init` takes the fallback; and a hold sink the host cannot read or fold launches nothing.

mod common;

use rigger::spawn::SpawnEvent;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use common::fixtures::{
    drop_the_log, fixture_script, no_emit, payloads_of_type, types_of, wait_until_for, CwdGuard,
    FailAppendContaining, LandsBehind, RETRIES_BY_ITSELF,
};
use common::is_running;
use common::repo::SILENT_PAST_THE_POLL;
use rigger::conductor::{AgentDriver, AgentFailure, AgentResult, Error, SpawnOpts};
use rigger::config::AgentDef;
use rigger::driver::claude_code::{framed, Driver};
use rigger::driver::replay::spawn_scratch_path;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore, ExpectedRevision};
use rigger::liveness;
use rigger::progress::{
    AgentProgress, ApiRetry, SpawnLaunched, SupervisorBeat, TYPE_AGENT_PROGRESS, TYPE_API_RETRY,
    TYPE_SPAWN_LAUNCHED,
};
use rigger::run::{RunHeld, HOLD_BY_HOST, TYPE_RUN_HELD};
use rigger::spawn::{self, TYPE_SPAWN_RESULT};
use serde_json::{json, Value};

/// The fixture agent replaying the recorded success stream.
const STREAM_AGENT: &str = "claude-code-stream-agent.sh";

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
            handed_env: Vec::new(),
            beat_source: None,
            hold_sink: None,
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

    /// [`Fixture::stopping_driver`] with this fixture's progress store as its beat source, as
    /// `run_cli` wires the host.
    fn supervised_driver(&self, script: &str, stop_grace: Duration) -> Driver<'_> {
        Driver {
            beat_source: Some(&self.progress_store),
            ..self.stopping_driver(script, stop_grace)
        }
    }

    /// Open a hold on the run this fixture's run store carries.
    fn hold_the_run(&self) {
        let held = serde_json::to_vec(&RunHeld::default()).unwrap();
        self.run_store
            .append(
                rigger::conductor::STREAM,
                ExpectedRevision::Any,
                &[Event::new(TYPE_RUN_HELD, held)],
            )
            .unwrap();
    }

    /// Land one supervisor beat, now, on the run [`opts`] names.
    fn beat(&self) {
        rigger::progress_store::append_stamped(
            &self.progress_store,
            &opts("").run_id,
            &SupervisorBeat::default(),
            "the test's beat",
        )
        .unwrap();
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

    /// Every `SpawnLaunched` record (open and closing alike) the progress store holds for the
    /// run [`opts`] names, in append order.
    fn launch_records(&self) -> Vec<rigger::progress::SpawnLaunched> {
        let events = rigger::progress::read_run(&self.progress_store, &opts("").run_id).unwrap();
        common::cli::spawn_launches(&events)
            .into_iter()
            .map(|(_, record)| record)
            .collect()
    }

    /// Every agent-progress activity line the progress store holds for the run [`opts`] names.
    fn activities(&self) -> Vec<String> {
        rigger::progress::read_run(&self.progress_store, &opts("").run_id)
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

    /// Every record the progress store holds for the run [`opts`] names, in append order.
    fn progress_stream(&self) -> Vec<Event> {
        rigger::progress::read_run(&self.progress_store, &opts("").run_id).unwrap()
    }

    /// The body of every `ApiRetry` the progress store holds for the run [`opts`] names.
    fn retry_bodies(&self) -> Vec<Value> {
        payloads_of_type(&self.progress_stream(), TYPE_API_RETRY)
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

/// An agent bounded to `secs` seconds of wall clock, so a session silent past them is STOPped.
fn wall_clock_bounded(secs: u64) -> AgentDef {
    AgentDef {
        max_wall_clock: Some(secs),
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
        .spawn(&wall_clock_bounded(1), "do the thing", o, &no_emit)
        .expect_err("a stream that never produces a result must not read as a success");
    let elapsed = started.elapsed();
    assert!(err.0.contains("stopped"), "{}", err.0);
    (err.0, elapsed)
}

#[test]
fn a_finished_spawn_reclaims_its_own_scratch_and_spares_an_unfinished_siblings() {
    // The per-spawn scratch lifecycle on this blocking host: the spawn's terminus reclaims the
    // `agent-scratch` leaf rigger names for it (`spawn_scratch_path`), through the one reclaim
    // authority - a spawn whose result is recorded keeps no scratch - while a sibling spawn of
    // the same run that has not finished keeps its own.
    let fx = Fixture::new();
    let root = fx.scratch_root.path().to_string_lossy().into_owned();
    let o = opts("u-scratch/adversary#0");
    let leaf = spawn_scratch_path(&root, &o.run_id, &o.id).unwrap();
    let sibling = spawn_scratch_path(&root, &o.run_id, "u-scratch/adjudicator#0").unwrap();
    for dir in [&leaf, &sibling] {
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::write(dir.join("target/build.log"), "built").unwrap();
    }

    fx.driver()
        .spawn(&AgentDef::default(), "do the thing", &o, &no_emit)
        .expect("the recorded stream ends in a result");

    assert!(!leaf.exists(), "the finished spawn's scratch is reclaimed");
    assert!(
        sibling.join("target/build.log").exists(),
        "an unfinished sibling spawn keeps its scratch"
    );
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
    // The session the spawn ran as - the one its launch record names - rides back for the
    // conductor to record, and a fresh launch continued none.
    assert_eq!(result.session_id, fx.launch_records()[0].session_id);
    assert!(!result.session_id.is_empty());
    assert_eq!(result.resumed_from, "");
}

#[test]
fn a_resumed_spawn_reports_the_session_it_continued() {
    let fx = Fixture::new();
    let o = SpawnOpts {
        resumed_from: "sess-prior".to_string(),
        resume_task: "fix exactly these".to_string(),
        ..opts("u104-stream/implementer#1")
    };

    let result = fx
        .driver()
        .spawn(&AgentDef::default(), "do the thing", &o, &no_emit)
        .expect("the recorded stream ends in a result");

    assert_eq!(result.session_id, "sess-prior");
    assert_eq!(result.resumed_from, "sess-prior");
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
    // log holds one result." The host answers a spawn its run has already ended from the
    // log, ahead of any launch, so the second call makes no result to write - prove it by
    // calling spawn() twice for the SAME id and asserting the run store still holds exactly
    // one SpawnResult. That record_result_unless_ended lands nothing beside a real result
    // standing is the spawn store's own row.
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
        "the second spawn is answered from the log and records no second result"
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

/// The spawn [`spawn_fails_classified`] launches.
const CLASSIFIED_SPAWN: &str = "u104-fail-class/implementer#0";

/// Where a seeded `StopFailure` record lands against the launch under test.
#[derive(Clone, Copy)]
enum Landing {
    /// Ahead of the launch's open record, so an earlier launch's.
    BeforeTheLaunch,
    /// Right behind the launch's open record, so the launch's own when run and spawn match.
    DuringTheLaunch,
}

/// The failure a spawn of `script` ends in, with a StopFailure record of `billing_error` landing on
/// the `(run, spawn)` of `seeded`, where it says (when given), asserting its class is `class`
/// (`why` says why it must be); returns the error text.
fn spawn_fails_classified(
    script: &str,
    seeded: Option<(&str, &str, Landing)>,
    class: AgentFailure,
    why: &str,
) -> String {
    let fx = Fixture::new();
    let record = |store: &dyn EventStore, run: &str, spawn: &str| {
        rigger::progress_store::record_stop_failure(
            store,
            run,
            &rigger::progress::StopFailure {
                spawn: spawn.to_string(),
                class: "billing_error".to_string(),
            },
        )
        .unwrap();
    };
    if let Some((run, spawn, Landing::BeforeTheLaunch)) = seeded {
        record(&fx.progress_store, run, spawn);
    }
    let landing = LandsBehind::new(
        &fx.progress_store,
        TYPE_SPAWN_LAUNCHED,
        move |store: &dyn EventStore| {
            if let Some((run, spawn, Landing::DuringTheLaunch)) = seeded {
                record(store, run, spawn);
            }
        },
    );
    let o = opts(CLASSIFIED_SPAWN); // run_id: "run-1"

    let err = Driver {
        progress_store: &landing,
        ..fx.driver_running(script)
    }
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
    // else the last `api_retry.error`". Land a StopFailure record for a DIFFERENT
    // category than the fixture's own api_retry line, as `rigger hook stop-failure` would
    // have written it during the launch, and prove the record wins.
    a_stopfailure_record_outranks_the_last_api_retry_category: spawn_fails_classified(
        "claude-code-api-retry-then-no-result-agent.sh",
        Some(("run-1", CLASSIFIED_SPAWN, Landing::DuringTheLaunch)),
        AgentFailure::BillingError,
        "the StopFailure record must outrank the api_retry category",
    );
    // Spec 105: the record reads only behind its launch's open record, so an earlier launch's
    // never classes this one, whatever its spawn and run.
    a_stopfailure_record_of_an_earlier_launch_never_classes_a_later_one: spawn_fails_classified(
        "claude-code-api-retry-then-no-result-agent.sh",
        Some(("run-1", CLASSIFIED_SPAWN, Landing::BeforeTheLaunch)),
        AgentFailure::AuthenticationFailed,
        "a StopFailure record ahead of this launch's open record must not class it",
    );
    // adv-u104c5-stopfailure-crosses-run-boundary: a StopFailure record left over from an
    // OLD or UNRELATED run must never outrank the LIVE session's own api_retry category -
    // the same `run_id` scoping `rigger status` already applies to this exact progress
    // stream (`src/main.rs`). The reciprocal of the sibling case just above: same spawn id,
    // same seeded class, but stamped on a DIFFERENT run than this session's own "run-1".
    a_stopfailure_record_from_a_different_run_does_not_outrank_the_live_sessions_api_retry:
        spawn_fails_classified(
            "claude-code-api-retry-then-no-result-agent.sh",
            Some(("some-other-run", CLASSIFIED_SPAWN, Landing::DuringTheLaunch)),
            AgentFailure::AuthenticationFailed,
            "the OTHER run's StopFailure record must not outrank this run's own api_retry \
             category",
        );
    // Spec 105's stray record: every launch carries the hook, a critic's included, and a critic
    // stamps the project's current run, so its record sits on this very run. It carries its own
    // spawn id, so this launch's class stays its own.
    a_stopfailure_record_of_another_spawn_on_this_run_does_not_class_this_launch:
        spawn_fails_classified(
            "claude-code-api-retry-then-no-result-agent.sh",
            Some((
                "run-1",
                &rigger::review::critique_spawn_id("spec-hash", 0),
                Landing::DuringTheLaunch,
            )),
            AgentFailure::AuthenticationFailed,
            "a critic's StopFailure record on this run must not class another spawn's launch",
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
    // starting its task, not winding down, so no grace is spent on it. The bound sits well
    // under the fixture's 30 s of work, so a host waiting it out still reds, and well over a
    // contended machine's start-up.
    assert!(
        elapsed < Duration::from_secs(15),
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
    // `record_result_unless_ended`, exactly like every other test in this file - THEN one
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
    // `record_result_unless_ended` durably recorded only the FIRST - a returned-value vs
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
        .spawn(&wall_clock_bounded(1), "do the thing", &o, &no_emit)
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

// ---- LIVENESS (spec 105): the stream-silence stop fires on silent time ----

/// The fixture announcing a 30 s retry and silent past a one-second bound inside it, then ending
/// in a result.
const RETRIES_PAST_THE_BOUND: &str = "claude-code-retries-silent-past-the-bound-agent.sh";

/// The fixture speaking a line each second for 5 s, then ending in a result.
const SPEAKS_PAST_THE_BOUND: &str = "claude-code-speaks-past-the-bound-agent.sh";

/// The fixture speaking a line each second for 4 s, then announcing a 30 s retry and silent for
/// 4 s inside it, then ending in a result.
const RETRIES_OLDER_THAN_THE_BOUND: &str = "claude-code-retries-older-than-the-bound-agent.sh";

/// The fixture announcing a 2 s retry and silent for 20 s, then ending in a result.
const HUNG_PAST_ITS_DELAY: &str = "claude-code-hung-past-its-delay-then-result-agent.sh";

/// How the stop's table sets a host and its run up.
#[derive(Clone, Copy, Debug)]
enum Beats {
    /// No beat source wired: the host reads no record at all.
    NoSource,
    /// A beat source with no beat on it: a run nobody supervised.
    None,
    /// A beat source with one fresh beat on it, so only the hold or the delay can excuse the
    /// silence.
    Fresh,
    /// A beat source whose log is gone, so every read of it fails.
    Unreadable,
}

/// A session under the bound of its row, in seconds, and a run set up as the row says: `Some` is
/// the result the session must end in, `None` is a stop.
#[test]
fn the_stream_silence_stop_fires_on_silent_time() {
    for (case, script, bound, held, beats, ended) in [
        (
            "a session silent across a hold reads through it",
            SILENT_PAST_THE_POLL,
            1,
            true,
            Beats::Fresh,
            Some("done: thought past the poll"),
        ),
        (
            "the same session with no hold is stopped",
            SILENT_PAST_THE_POLL,
            1,
            false,
            Beats::Fresh,
            None,
        ),
        (
            "a session inside the delay it announced reads through it",
            RETRIES_PAST_THE_BOUND,
            1,
            false,
            Beats::Fresh,
            Some("done: waited the delay out"),
        ),
        (
            "a run with no beats is unsupervised",
            SILENT_PAST_THE_POLL,
            1,
            false,
            Beats::None,
            Some("done: thought past the poll"),
        ),
        (
            "a wake whose records cannot be read counts no silence",
            SILENT_PAST_THE_POLL,
            1,
            false,
            Beats::Unreadable,
            Some("done: thought past the poll"),
        ),
        (
            "a host with no beat source counts from the last line, whatever the hold",
            SILENT_PAST_THE_POLL,
            1,
            true,
            Beats::NoSource,
            None,
        ),
        (
            "a host with no beat source counts from the last line, whatever the delay",
            RETRIES_PAST_THE_BOUND,
            1,
            false,
            Beats::NoSource,
            None,
        ),
        (
            "a host with no beat source reads a speaking session through",
            SPEAKS_PAST_THE_BOUND,
            3,
            false,
            Beats::NoSource,
            Some("done: spoke past the bound"),
        ),
        (
            "a session older than its bound counts from its last line, never its launch",
            RETRIES_OLDER_THAN_THE_BOUND,
            3,
            false,
            Beats::Fresh,
            Some("done: waited the delay out at its age"),
        ),
        (
            "a session silent past the delay it announced is stopped on a later wake",
            HUNG_PAST_ITS_DELAY,
            1,
            false,
            Beats::Fresh,
            None,
        ),
    ] {
        let fx = Fixture::new();
        if held {
            fx.hold_the_run();
        }
        let grace = Duration::from_millis(300);
        let broken_db = fx.scratch_root.path().join("broken-progress.db");
        let broken = Store::open(broken_db.to_str().unwrap()).unwrap();
        drop_the_log(&broken_db);
        let driver = match beats {
            Beats::NoSource => fx.stopping_driver(script, grace),
            Beats::None => fx.supervised_driver(script, grace),
            Beats::Fresh => {
                fx.beat();
                fx.supervised_driver(script, grace)
            }
            Beats::Unreadable => Driver {
                beat_source: Some(&broken),
                ..fx.stopping_driver(script, grace)
            },
        };
        let o = opts("u105-silent/implementer#0");

        let spawned = driver.spawn(&wall_clock_bounded(bound), "do the thing", &o, &no_emit);

        assert_eq!(
            spawned.as_ref().ok().map(|r| r.output.as_str()),
            ended,
            "{case}: {:?}",
            spawned.as_ref().map(|r| &r.output)
        );
        let stops = fx
            .launch_records()
            .iter()
            .filter(|l| l.ended.as_deref() == Some("stopped"))
            .count();
        assert_eq!(stops, usize::from(ended.is_none()), "{case}");
        let results = fx.spawn_results();
        assert_eq!(results.len(), 1, "{case}: one result landed");
        assert_eq!(
            spawn::SpawnResult::from_event(&results[0])
                .unwrap()
                .is_liveness_fault(),
            ended.is_none(),
            "{case}"
        );
    }
}

#[test]
fn a_bounded_launch_held_past_its_bound_still_sees_its_child_exit_behind_a_held_pipe() {
    // Silent time frozen by a hold never reaches the bound, so with no EOF (a descendant still
    // holds the stdout pipe) the wake-up re-checking the child is the only thing that sees the
    // session end, as on an unbounded launch.
    let fx = Fixture::new();
    fx.hold_the_run();
    fx.beat();
    let started = Instant::now();
    let err = fx
        .supervised_driver(
            "claude-code-exits-leaving-a-descendant-on-the-pipe-agent.sh",
            Duration::from_millis(300),
        )
        .spawn(
            &wall_clock_bounded(1),
            "do the thing",
            &opts("u105-exited/implementer#0"),
            &no_emit,
        )
        .expect_err("a session that exited with no result is no success");
    assert!(err.0.contains("ended with no result"), "{}", err.0);
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "the read ends at the wake-up after the child exits: {:?}",
        started.elapsed()
    );
}

// ---- LIVENESS (spec 105): the `ApiRetry` record a real session's `api_retry` lines leave ----

/// The exact bodies the retries `announced` leave on record for spawn `spawn`, in order.
fn expected_retries(spawn: &str, announced: &[(u64, &str)]) -> Vec<Value> {
    announced
        .iter()
        .map(|(delay_ms, category)| {
            json!({"spawn": spawn, "delay_ms": delay_ms, "category": category})
        })
        .collect()
}

/// A session announcing two retries, over a progress store refusing every append whose body
/// holds `refused`: the session ends in its result with its waiting lines on record either way,
/// its run's progress stream reads exactly `types`, its launch's open and `completed` records
/// around its lines, and exactly `retries` are on record.
fn a_session_announcing_two_retries(
    refused: &'static str,
    types: &[&str],
    retries: &[(u64, &str)],
) {
    let fx = Fixture::new();
    let o = opts("u105-retry/implementer#0");
    let refusing = FailAppendContaining {
        inner: &fx.progress_store,
        needle: refused,
    };
    let driver = Driver {
        progress_store: &refusing,
        ..fx.driver_running("claude-code-two-api-retries-agent.sh")
    };

    let result = driver
        .spawn(&AgentDef::default(), "do the thing", &o, &no_emit)
        .expect("a session ending in a result succeeds");

    assert_eq!(result.output, "done after two retries");
    assert_eq!(fx.spawn_results().len(), 1);
    assert_eq!(types_of(&fx.progress_stream()), types);
    assert_eq!(fx.retry_bodies(), expected_retries(&o.id, retries));
    assert_eq!(
        fx.activities(),
        [
            "mcp servers: []",
            "waiting: rate_limit (attempt 1, retrying in 618ms)",
            "waiting: overloaded (attempt 2, retrying in 30000ms)",
        ]
    );
}

rigger::test_cases! {
    each_announced_retry_lands_beside_its_waiting_line_in_the_runs_progress_stream:
        a_session_announcing_two_retries(
            "a body no record holds",
            &[
                TYPE_SPAWN_LAUNCHED,
                TYPE_AGENT_PROGRESS,
                TYPE_AGENT_PROGRESS,
                TYPE_API_RETRY,
                TYPE_AGENT_PROGRESS,
                TYPE_API_RETRY,
                TYPE_SPAWN_LAUNCHED,
            ],
            &[(618, "rate_limit"), (30_000, "overloaded")],
        );
    // Only an `ApiRetry` body spells `delay_ms`, so this store loses those two records and
    // nothing else: a lost record only leaves the silence it would have explained counted,
    // where failing the session would throw its work away.
    a_refused_retry_record_costs_the_session_neither_its_result_nor_its_waiting_lines:
        a_session_announcing_two_retries(
            "delay_ms",
            &[
                TYPE_SPAWN_LAUNCHED,
                TYPE_AGENT_PROGRESS,
                TYPE_AGENT_PROGRESS,
                TYPE_AGENT_PROGRESS,
                TYPE_SPAWN_LAUNCHED,
            ],
            &[],
        );
}

#[test]
fn a_retry_is_on_record_while_its_session_still_waits_it_out() {
    // Liveness reads the record during the silence the delay explains, so one reaching the
    // store only as the session ends would explain nothing. The fixture cannot end before this
    // test has read the record: it stays silent until the release file exists.
    let fx = Fixture::new();
    let release = fx.scratch_file("record-read");
    let o = opts_with_env(
        "u105-retry/implementer#0",
        "RIGGER_TEST_RETRY_RELEASE_FILE",
        &release,
    );
    let driver = fx.driver_running("claude-code-waits-out-an-api-retry-agent.sh");
    let announced = expected_retries(&o.id, &[(30_000, "overloaded")]);

    let (on_record_inside_the_wait, session) = std::thread::scope(|scope| {
        let session =
            scope.spawn(|| driver.spawn(&AgentDef::default(), "do the thing", &o, &no_emit));
        let on_record = wait_until_for(800, || fx.retry_bodies() == announced);
        std::fs::write(&release, "").unwrap();
        (on_record, session.join().unwrap())
    });

    assert_eq!(fx.retry_bodies(), announced);
    assert!(
        on_record_inside_the_wait,
        "the record must reach the store while the session is silent inside the delay"
    );
    assert_eq!(
        session
            .expect("the released session ends in its result")
            .output,
        "done after the wait"
    );
}

/// A session of `script` ending with no result keeps the exact `retries` its lines announced on
/// record, and fails with the exact no-result error naming `class`.
fn a_session_with_no_result_keeps_its_retries(
    script: &str,
    retries: &[(u64, &str)],
    class: AgentFailure,
) {
    let fx = Fixture::new();
    let o = opts("u105-retry/implementer#0");

    let err = fx
        .driver_running(script)
        .spawn(&AgentDef::default(), "do the thing", &o, &no_emit)
        .expect_err("a session ending with no result must not succeed");

    assert_eq!(fx.retry_bodies(), expected_retries(&o.id, retries));
    assert_eq!(
        rigger::conductor::strip_failure_marker(&err),
        format!(
            "claude_code driver: {:?}: the agent stream ended with no result \
             (class {class}; stderr tail: )",
            o.id
        )
    );
}

rigger::test_cases! {
    a_session_ending_on_a_retry_keeps_it_on_record:
        a_session_with_no_result_keeps_its_retries(
            "claude-code-api-retry-then-no-result-agent.sh",
            &[(500, "authentication_failed")],
            AgentFailure::AuthenticationFailed,
        );
    // A delay that is no count reads 0 and a missing category reads `unknown` on the record,
    // and that floor never reaches the class: the session's last line names no category, so
    // its class stays the last one a line did name.
    a_sparse_retry_line_records_its_floors_and_leaves_the_class_on_the_last_named_category:
        a_session_with_no_result_keeps_its_retries(
            "claude-code-sparse-api-retries-then-no-result-agent.sh",
            &[
                (1500, "a_category_no_binary_knows"),
                (0, "overloaded"),
                (618, "rate_limit"),
                (0, "unknown"),
            ],
            AgentFailure::RateLimit,
        );
}

/// The on-disk `body` decodes to the three-field record of the retry it announced.
fn a_retry_body_decodes(body: &str) {
    assert_eq!(
        serde_json::from_str::<ApiRetry>(body).unwrap(),
        ApiRetry {
            spawn: "u9/implementer#0".to_string(),
            delay_ms: 618,
            category: "rate_limit".to_string(),
        }
    );
}

rigger::test_cases! {
    an_api_retry_body_decodes_as_a_writer_leaves_it: a_retry_body_decodes(
        r#"{"spawn":"u9/implementer#0","delay_ms":618,"category":"rate_limit"}"#,
    );
    // A later binary adding a key must not cost an earlier one the rest of the record.
    an_api_retry_body_decodes_past_a_key_a_later_writer_added: a_retry_body_decodes(
        r#"{"spawn":"u9/implementer#0","delay_ms":618,"category":"rate_limit","attempt":3}"#,
    );
}

// ---- THE HOLD (spec 105): the table's transitions, through a real child ----

/// The fixture agent printing the stream lines a test scripted, then ending as the real host does.
const SCRIPTED_AGENT: &str = "claude-code-scripted-stream-agent.sh";

/// The spawn every single-launch row of the hold tests drives.
const HELD_SPAWN: &str = "u105-table/implementer#0";

/// `system/init` reporting the spawn's server connected, on a resolved model no hold may name.
fn init_line() -> String {
    json!({
        "type": "system",
        "subtype": "init",
        "session_id": "scripted",
        "model": "the-resolved-model",
        "mcp_servers": [{"name": "rigger", "status": "connected"}],
    })
    .to_string()
}

/// A `system/api_retry` line carrying `category`.
fn retry_line(category: &str) -> String {
    json!({
        "type": "system",
        "subtype": "api_retry",
        "attempt": 1,
        "max_retries": 10,
        "retry_delay_ms": 1,
        "error_status": 500,
        "error": category,
    })
    .to_string()
}

/// The `assistant` line an API error prints, its `error` carrying `category`.
fn assistant_error_line(category: &str) -> String {
    json!({
        "type": "assistant",
        "message": {"model": "<synthetic>", "content": []},
        "error": category,
        "is_api_error_message": true,
    })
    .to_string()
}

/// A `result` line, whose `is_error` is `is_error`.
fn result_line(is_error: bool) -> String {
    json!({
        "type": "result",
        "subtype": "success",
        "is_error": is_error,
        "num_turns": 1,
        "session_id": "scripted",
        "result": "the answer",
    })
    .to_string()
}

/// A hold of `cause` by the host, on the launch's `model`.
fn hold(cause: &str, detail: &str, action: &str, needs_person: bool, model: &str) -> RunHeld {
    RunHeld {
        cause: cause.to_string(),
        detail: detail.to_string(),
        action: action.to_string(),
        needs_person,
        by: HOLD_BY_HOST.to_string(),
        model: model.to_string(),
    }
}

/// A hold of `authentication_failed`, sighted on an `api_retry` of the default model.
fn login_hold() -> RunHeld {
    let cause = "authentication_failed";
    hold(cause, cause, "run claude auth login", true, "")
}

/// The third-consecutive hold: no line sighted it and nobody needs to act.
fn unstable_hold() -> RunHeld {
    hold("api-unstable", "", RETRIES_BY_ITSELF, false, "")
}

/// A `RunHeld` event by the operator.
fn operator_hold() -> Event {
    common::fixtures::held_event("operator", "operator")
}

impl Fixture {
    /// This fixture with a started run on its run stream, and `log` behind the `RunStarted`.
    fn started(log: &[Event]) -> Self {
        let fx = Fixture::new();
        rigger::run_store::ensure_started(&fx.run_store, &[]).unwrap();
        if !log.is_empty() {
            fx.run_store
                .append(rigger::conductor::STREAM, ExpectedRevision::Any, log)
                .unwrap();
        }
        fx
    }

    /// Everything on the run stream, oldest first.
    fn run_log(&self) -> Vec<Event> {
        self.run_store
            .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
            .unwrap()
    }

    /// Every `RunHeld` on the run stream, oldest first.
    fn holds(&self) -> Vec<RunHeld> {
        payloads_of_type(&self.run_log(), TYPE_RUN_HELD)
            .into_iter()
            .map(|body| serde_json::from_value(body).unwrap())
            .collect()
    }

    /// [`Fixture::driver_running`] of `script` with this fixture's run store as its hold sink and
    /// a grace a fixture that winds down on input close never waits out.
    fn held_driver(&self, script: &str) -> Driver<'_> {
        Driver {
            hold_sink: Some(&self.run_store),
            stop_grace: Duration::from_secs(5),
            ..self.driver_running(script)
        }
    }

    /// Plant a build log in `o`'s scratch leaf, the work a launch leaves there: the file's path.
    fn plant_scratch(&self, o: &SpawnOpts) -> PathBuf {
        let root = self.scratch_root.path().to_string_lossy();
        let leaf = spawn_scratch_path(&root, &o.run_id, &o.id).unwrap();
        let planted = leaf.join("target/build.log");
        std::fs::create_dir_all(planted.parent().unwrap()).unwrap();
        std::fs::write(&planted, "built").unwrap();
        planted
    }

    /// How many closes of the run the progress log counts toward `api-unstable`.
    fn failed_closes(&self) -> usize {
        rigger::progress::failed_closes_since(&self.progress_stream(), None)
    }

    /// [`opts`] for `spawn` whose environment hands the scripted agent `lines`.
    fn scripted(&self, spawn: &str, lines: &[String]) -> SpawnOpts {
        let file = self.scratch_file(&format!("{}.jsonl", spawn.replace(['/', '#'], "-")));
        std::fs::write(&file, lines.join("\n") + "\n").unwrap();
        opts_with_env(spawn, "RIGGER_TEST_SCRIPTED_STREAM", &file)
    }
}

/// The wall clock a scripted session is bounded to, far past its life, so a regression that leaves
/// a session's input open reads as a stopped launch, never a hung test.
const SCRIPTED_WALL_CLOCK_S: u64 = 5;

/// One launch of a [`hold_cases`] row: its wire and its world, then what it leaves.
struct HoldCase {
    name: &'static str,
    lines: Vec<String>,
    /// What stands on the run stream behind its `RunStarted` before the launch.
    seeded: Vec<Event>,
    agent: AgentDef,
    attempt: u32,
    /// Whether the host is built with no hold sink.
    no_sink: bool,
    /// The class of a `StopFailure` record landing right behind the launch's open record.
    stop_failure: Option<&'static str>,
    /// The `RunHeld`s the launch appends, in order.
    holds: Vec<RunHeld>,
    /// How many `SpawnResult`s the launch records.
    results: usize,
    /// The launch's close: its `ended`, then its class.
    close: (&'static str, Option<&'static str>),
    /// The index in the run log of the event whose revision the close carries, the log holding
    /// `RunStarted`, then `seeded`, then the `SpawnRequested` the host parks ahead of its launch.
    stamp: Option<usize>,
}

impl Default for HoldCase {
    fn default() -> Self {
        HoldCase {
            name: "",
            lines: Vec::new(),
            seeded: Vec::new(),
            agent: wall_clock_bounded(SCRIPTED_WALL_CLOCK_S),
            attempt: 0,
            no_sink: false,
            stop_failure: None,
            holds: Vec::new(),
            results: 0,
            close: ("", None),
            stamp: None,
        }
    }
}

/// Every launch the table decides. The run log every `stamp` indexes starts at the `RunStarted`.
fn hold_cases() -> Vec<HoldCase> {
    let login = "authentication_failed";
    let billing = "fix the account's billing";
    let model_action =
        "fix the agent's configured model, restart rigger run with --rebase-definition, then run \
         rigger release";
    let model_ending = hold("model_not_found", "", model_action, true, "");
    let wait_ending = hold("rate_limit", "", RETRIES_BY_ITSELF, false, "");
    vec![
        HoldCase {
            name: "a hold-class retry the session recovers from",
            lines: vec![init_line(), retry_line(login), result_line(false)],
            holds: vec![login_hold()],
            results: 1,
            close: ("completed", None),
            stamp: Some(3),
            ..HoldCase::default()
        },
        HoldCase {
            name: "a repeated sighting",
            lines: vec![
                init_line(),
                retry_line(login),
                retry_line(login),
                result_line(false),
            ],
            holds: vec![login_hold()],
            results: 1,
            close: ("completed", None),
            stamp: Some(3),
            ..HoldCase::default()
        },
        HoldCase {
            name: "a differing person-needing category",
            lines: vec![
                init_line(),
                retry_line(login),
                retry_line("billing_error"),
                result_line(false),
            ],
            holds: vec![
                login_hold(),
                hold("billing_error", "billing_error", billing, true, ""),
            ],
            results: 1,
            close: ("completed", None),
            stamp: Some(4),
            ..HoldCase::default()
        },
        HoldCase {
            name: "a verification category holds on its row",
            lines: vec![
                init_line(),
                retry_line("verification_required"),
                result_line(false),
            ],
            holds: vec![hold(
                "verification_required",
                "verification_required",
                "complete the account's verification",
                true,
                "",
            )],
            results: 1,
            close: ("completed", None),
            stamp: Some(3),
            ..HoldCase::default()
        },
        HoldCase {
            name: "a differing wait-class retry on a hold",
            lines: vec![
                init_line(),
                retry_line(login),
                retry_line("rate_limit"),
                result_line(false),
            ],
            holds: vec![login_hold()],
            results: 1,
            close: ("completed", None),
            stamp: Some(3),
            ..HoldCase::default()
        },
        HoldCase {
            name: "a wait-class retry the session recovers from",
            lines: vec![init_line(), retry_line("rate_limit"), result_line(false)],
            results: 1,
            close: ("completed", None),
            stamp: Some(2),
            ..HoldCase::default()
        },
        HoldCase {
            name: "a session ending of a wait class",
            lines: vec![init_line(), retry_line("rate_limit")],
            holds: vec![wait_ending.clone()],
            close: ("interrupted", Some("rate_limit")),
            stamp: Some(1),
            ..HoldCase::default()
        },
        HoldCase {
            name: "a session ending of the hold class it sighted",
            lines: vec![init_line(), retry_line(login)],
            holds: vec![login_hold()],
            close: ("interrupted", Some(login)),
            stamp: Some(2),
            ..HoldCase::default()
        },
        HoldCase {
            name: "a session ending of a wait class on a hold",
            lines: vec![init_line(), retry_line(login), retry_line("rate_limit")],
            holds: vec![login_hold()],
            close: ("interrupted", Some("rate_limit")),
            stamp: Some(2),
            ..HoldCase::default()
        },
        HoldCase {
            name: "a session ending of a fault class",
            lines: vec![init_line(), retry_line("invalid_request")],
            close: ("fault", Some("invalid_request")),
            stamp: Some(1),
            ..HoldCase::default()
        },
        HoldCase {
            name: "a category no binary knows",
            lines: vec![init_line(), retry_line("a_category_no_binary_knows")],
            close: ("fault", Some("unknown")),
            stamp: Some(1),
            ..HoldCase::default()
        },
        HoldCase {
            name: "an is_error result, classed after the exit from the assistant line",
            lines: vec![
                init_line(),
                assistant_error_line("model_not_found"),
                result_line(true),
            ],
            holds: vec![model_ending.clone()],
            close: ("interrupted", Some("model_not_found")),
            stamp: Some(1),
            ..HoldCase::default()
        },
        HoldCase {
            name: "an is_error result a later result never answers",
            lines: vec![
                init_line(),
                assistant_error_line("rate_limit"),
                result_line(true),
                result_line(false),
            ],
            holds: vec![wait_ending],
            close: ("interrupted", Some("rate_limit")),
            stamp: Some(1),
            ..HoldCase::default()
        },
        HoldCase {
            name: "an is_error result nothing classes",
            lines: vec![init_line(), result_line(true)],
            close: ("fault", Some("unknown")),
            stamp: Some(1),
            ..HoldCase::default()
        },
        HoldCase {
            name: "a clean assistant turn clears the error ahead of it",
            lines: vec![
                init_line(),
                assistant_error_line("model_not_found"),
                r#"{"type":"assistant","message":{"role":"assistant","content":[]}}"#.to_string(),
                result_line(true),
            ],
            close: ("fault", Some("unknown")),
            stamp: Some(1),
            ..HoldCase::default()
        },
        HoldCase {
            name: "a StopFailure record outranks the retry it follows",
            lines: vec![init_line(), retry_line(login)],
            stop_failure: Some("billing_error"),
            holds: vec![login_hold(), hold("billing_error", "", billing, true, "")],
            close: ("interrupted", Some("billing_error")),
            stamp: Some(2),
            ..HoldCase::default()
        },
        HoldCase {
            name: "the assistant line outranks a StopFailure record",
            lines: vec![
                init_line(),
                assistant_error_line("model_not_found"),
                result_line(true),
            ],
            stop_failure: Some("billing_error"),
            holds: vec![model_ending],
            close: ("interrupted", Some("model_not_found")),
            stamp: Some(1),
            ..HoldCase::default()
        },
        HoldCase {
            name: "a sighting on an operator hold",
            lines: vec![init_line(), retry_line(login), result_line(false)],
            seeded: vec![operator_hold()],
            results: 1,
            close: ("completed", None),
            stamp: Some(3),
            ..HoldCase::default()
        },
        HoldCase {
            name: "a wait-class ending on an operator hold",
            lines: vec![init_line(), retry_line("rate_limit")],
            seeded: vec![operator_hold()],
            close: ("interrupted", Some("rate_limit")),
            stamp: Some(2),
            ..HoldCase::default()
        },
        HoldCase {
            name: "a fault-class ending on an operator hold",
            lines: vec![init_line(), retry_line("invalid_request")],
            seeded: vec![operator_hold()],
            close: ("interrupted", Some("invalid_request")),
            stamp: Some(2),
            ..HoldCase::default()
        },
        HoldCase {
            name: "an unclassed ending on an operator hold",
            lines: vec![init_line(), result_line(true)],
            seeded: vec![operator_hold()],
            close: ("interrupted", Some("unknown")),
            stamp: Some(2),
            ..HoldCase::default()
        },
        HoldCase {
            name: "a fault-class ending on a hold released ahead of it",
            lines: vec![init_line(), retry_line("invalid_request")],
            seeded: vec![
                operator_hold(),
                common::fixtures::released_event("operator", 0),
            ],
            close: ("fault", Some("invalid_request")),
            stamp: Some(3),
            ..HoldCase::default()
        },
        HoldCase {
            name: "a fault-class ending on a host with no hold sink",
            lines: vec![init_line(), retry_line("invalid_request")],
            seeded: vec![operator_hold()],
            no_sink: true,
            close: ("fault", Some("invalid_request")),
            ..HoldCase::default()
        },
        HoldCase {
            name: "a hold-class retry on a host with no hold sink",
            lines: vec![init_line(), retry_line(login), result_line(false)],
            no_sink: true,
            results: 1,
            close: ("completed", None),
            ..HoldCase::default()
        },
        HoldCase {
            name: "a wait-class ending on a host with no hold sink",
            lines: vec![init_line(), retry_line("rate_limit")],
            no_sink: true,
            close: ("fault", Some("rate_limit")),
            ..HoldCase::default()
        },
        HoldCase {
            name: "a hold names the agent's configured model",
            lines: vec![init_line(), retry_line(login), result_line(false)],
            agent: AgentDef {
                model: "sonnet".to_string(),
                ..wall_clock_bounded(SCRIPTED_WALL_CLOCK_S)
            },
            holds: vec![RunHeld {
                model: "sonnet".to_string(),
                ..login_hold()
            }],
            results: 1,
            close: ("completed", None),
            stamp: Some(3),
            ..HoldCase::default()
        },
        HoldCase {
            name: "a hold names the rung its launch ran on",
            lines: vec![init_line(), retry_line(login), result_line(false)],
            agent: AgentDef {
                model_ladder: vec!["sonnet".to_string(), "opus".to_string()],
                ..wall_clock_bounded(SCRIPTED_WALL_CLOCK_S)
            },
            attempt: 1,
            holds: vec![RunHeld {
                model: "opus".to_string(),
                ..login_hold()
            }],
            results: 1,
            close: ("completed", None),
            stamp: Some(3),
            ..HoldCase::default()
        },
    ]
}

/// THE DISPOSITION TABLE through a real child: each row's launch appends exactly its holds, ends
/// its spawn as its close says, records its results and stamps the revision it read on its close
/// alone, never on its open record. A launch closing `interrupted` keeps its scratch and enters no
/// failed-close count; a `fault` close counts once and reclaims it, and a `completed` close counts
/// nothing and reclaims it.
#[cfg(unix)]
#[test]
fn a_launch_appends_the_holds_the_table_decides_and_closes_where_it_read() {
    for case in hold_cases() {
        let name = case.name;
        let fx = Fixture::started(&case.seeded);
        let before = fx.holds().len();
        let landing = LandsBehind::new(
            &fx.progress_store,
            TYPE_SPAWN_LAUNCHED,
            move |store: &dyn EventStore| {
                if let Some(class) = case.stop_failure {
                    rigger::progress_store::record_stop_failure(
                        store,
                        "run-1",
                        &rigger::progress::StopFailure {
                            spawn: HELD_SPAWN.to_string(),
                            class: class.to_string(),
                        },
                    )
                    .unwrap();
                }
            },
        );
        let driver = Driver {
            progress_store: &landing,
            hold_sink: (!case.no_sink).then_some(&fx.run_store as &dyn EventStore),
            ..fx.held_driver(SCRIPTED_AGENT)
        };
        let o = SpawnOpts {
            attempt: case.attempt,
            ..fx.scripted(HELD_SPAWN, &case.lines)
        };
        let planted = fx.plant_scratch(&o);

        let outcome = driver.spawn(&case.agent, "do the thing", &o, &no_emit);

        let (ended, class) = case.close;
        match (ended, &outcome) {
            ("completed", Ok(result)) => assert_eq!(result.output, "the answer", "{name}"),
            ("fault" | "interrupted", Err(e)) => assert!(
                rigger::conductor::strip_failure_marker(e)
                    .contains(&format!("class {};", class.unwrap())),
                "{name}: {}",
                e.0
            ),
            (_, other) => panic!("{name}: ended {other:?}"),
        }
        assert_eq!(fx.holds()[before..], case.holds, "{name}: the holds");
        assert_eq!(
            fx.spawn_results().len(),
            case.results,
            "{name}: the results"
        );
        let closing = fx.launch_records().pop().unwrap();
        assert_eq!(
            (closing.ended.as_deref(), closing.class.as_deref()),
            (Some(ended), class),
            "{name}: the close"
        );
        assert_eq!(
            closing.revision,
            case.stamp.map(|at| fx.run_log()[at].revision),
            "{name}: the close's stamp"
        );
        let opening = fx.launch_records().into_iter().rev().nth(1).unwrap();
        assert_eq!(
            (opening.ended.as_deref(), opening.revision),
            (None, None),
            "{name}: the open record, which no stamp reaches"
        );
        assert_eq!(
            planted.exists(),
            ended == "interrupted",
            "{name}: the scratch"
        );
        assert_eq!(
            fx.failed_closes(),
            usize::from(ended == "fault"),
            "{name}: the failed closes"
        );
    }
}

/// One launch of a [`a_third_consecutive_failed_close_holds_the_run`] sequence.
#[derive(Clone, Copy, Debug)]
enum Step {
    /// A launch ending with no result: a `fault` close of no class.
    Fault,
    /// A silent launch the wall clock stops: a `stopped` close.
    Stop,
    /// A launch answering: a `completed` close.
    Completed,
    /// A release of the open hold, with no launch.
    Release,
}

impl Fixture {
    /// Take `step`, the `at`th of its sequence, each launch a spawn of its own.
    fn take(&self, step: Step, at: usize) {
        let spawn = format!("u105-seq/implementer#{at}");
        let scripted = || self.held_driver(SCRIPTED_AGENT);
        match step {
            Step::Fault => {
                let o = self.scripted(&spawn, &[init_line()]);
                let spawned = scripted().spawn(
                    &wall_clock_bounded(SCRIPTED_WALL_CLOCK_S),
                    "do the thing",
                    &o,
                    &no_emit,
                );
                spawned.expect_err("a session with no result fails its spawn");
            }
            Step::Stop => {
                let driver = self.held_driver("claude-code-silent-winds-down-on-eof-agent.sh");
                stop(&driver, &opts(&spawn));
            }
            Step::Completed => {
                let o = self.scripted(&spawn, &[init_line(), result_line(false)]);
                let spawned = scripted().spawn(
                    &wall_clock_bounded(SCRIPTED_WALL_CLOCK_S),
                    "do the thing",
                    &o,
                    &no_emit,
                );
                spawned.expect("a session with a result answers its spawn");
            }
            Step::Release => {
                rigger::run_store::release_hold(&self.run_store, rigger::run::HOLD_BY_HOST)
                    .map(drop)
                    .unwrap()
            }
        }
    }
}

/// The third consecutive fault or stream-silence stop holds the run `api-unstable`, before the
/// host returns its error, once per hold: a `completed` close or a `RunReleased` restarts the
/// count, and a hold released is held again by three more.
#[cfg(unix)]
#[test]
fn a_third_consecutive_failed_close_holds_the_run() {
    use Step::{Completed as C, Fault as F, Release as R, Stop as S};
    let rows: [(&str, Vec<Step>, usize); 10] = [
        ("two faults", vec![F, F], 0),
        ("three faults", vec![F, F, F], 1),
        ("a stop between faults", vec![F, S, F], 1),
        ("three stops", vec![S, S, S], 1),
        ("four faults", vec![F, F, F, F], 1),
        ("a completed close restarts it", vec![F, F, C, F, F], 0),
        ("a completed close behind two", vec![C, F, F, F], 1),
        ("a release restarts it", vec![F, F, F, R, F, F], 1),
        ("three more after a release", vec![F, F, F, R, F, F, F], 2),
        ("a completed close alone", vec![C], 0),
    ];
    for (name, steps, expected) in rows {
        let fx = Fixture::started(&[]);
        for (at, step) in steps.into_iter().enumerate() {
            fx.take(step, at);
        }
        assert_eq!(fx.holds(), vec![unstable_hold(); expected], "{name}");
    }
}

/// A fault on a run an operator holds closes `interrupted` ahead of the count: the two failed closes
/// before it stand at two, and no `api-unstable` hold joins the operator's.
#[cfg(unix)]
#[test]
fn a_close_on_a_held_run_enters_no_unstable_count() {
    let fx = Fixture::started(&[]);
    fx.take(Step::Fault, 0);
    fx.take(Step::Fault, 1);
    let before = fx.run_log().len();
    fx.run_store
        .append(
            rigger::conductor::STREAM,
            ExpectedRevision::Any,
            &[operator_hold()],
        )
        .unwrap();

    fx.take(Step::Fault, 2);

    assert_eq!(fx.failed_closes(), 2);
    assert_eq!(
        fx.run_log()[before..]
            .iter()
            .map(|event| event.type_.as_str())
            .collect::<Vec<_>>(),
        vec![TYPE_RUN_HELD, spawn::TYPE_SPAWN_REQUESTED],
        "nothing but the third spawn's park lands past the hold"
    );
    let closing = fx.launch_records().pop().unwrap();
    assert_eq!(
        (closing.ended.as_deref(), closing.class.as_deref()),
        (Some("interrupted"), Some("unknown"))
    );
}

/// Closes racing past the third append one hold between them: each writes its close, re-reads the
/// count and compares its append against the stream.
#[cfg(unix)]
#[test]
fn concurrent_closes_past_the_third_hold_the_run_once() {
    let fx = Fixture::started(&[]);
    fx.take(Step::Fault, 0);
    fx.take(Step::Fault, 1);
    std::thread::scope(|scope| {
        for at in 2..6 {
            let fx = &fx;
            scope.spawn(move || fx.take(Step::Fault, at));
        }
    });
    assert_eq!(fx.holds(), vec![unstable_hold()]);
}

/// The fixture agent ending a `--resume` launch as `RIGGER_TEST_RESUME_ENDING` and any other
/// launch as `RIGGER_TEST_FRESH_ENDING`.
const RESUME_ENDING_AGENT: &str = "claude-code-resume-ending-agent.sh";

/// The spawn the fallback tests launch, a re-attempt.
const RESUMED_SPAWN: &str = "u105-resume/implementer#1";

/// What the host's failure text for [`RESUMED_SPAWN`] opens with.
const PORT_PREFIX: &str = "claude_code driver: \"u105-resume/implementer#1\": ";

/// The port's failure for [`RESUMED_SPAWN`] when a session ended with no result and nothing on
/// stderr.
const NO_RESULT_FAILURE: &str = "claude_code driver: \"u105-resume/implementer#1\": the agent \
    stream ended with no result (class unknown; stderr tail: )";

/// The port's failure for [`RESUMED_SPAWN`] when a session ended with no result and the refusal's
/// tell on stderr.
const REFUSED_FAILURE: &str = "claude_code driver: \"u105-resume/implementer#1\": the agent \
    stream ended with no result (class unknown; stderr tail: No conversation found with session \
    ID: sess-prior)";

/// The port's failure for [`RESUMED_SPAWN`] when its `rigger` server reported `failed`.
const SERVER_FAILURE: &str = "claude_code driver: \"u105-resume/implementer#1\": its rigger MCP \
    server reported status \"failed\" at init, not connected - the session was stopped so its \
    task never runs without the rigger tools (class unknown)";

/// The port's failure for [`RESUMED_SPAWN`] when it stayed silent past [`SILENCE_BOUND`].
const SILENCE_FAILURE: &str = "claude_code driver: \"u105-resume/implementer#1\": spawn \
    \"u105-resume/implementer#1\" stopped: its agent stream stayed silent for 3s (its \
    max_wall_clock bound) with no result - no remediation attempt is charged (the unit's code is \
    not at fault). Re-drive it once the agent/driver is healthy: record a real result with \
    `rigger result u105-resume/implementer#1` (last-write-wins supersedes this liveness fault).";

/// The port's failure for [`RESUMED_SPAWN`] when the host could not read a line.
const UNREADABLE_FAILURE: &str = "claude_code driver: \"u105-resume/implementer#1\": read agent \
    stream: stream did not contain valid UTF-8";

/// [`RESUMED_SPAWN`] as the stem of its transcript files.
const RESUMED_STEM: &str = "u105-resume_2fimplementer_231";

/// The session [`RESUMED_SPAWN`] asks to continue.
const PRIOR_SESSION: &str = "sess-prior";

/// The first message of a `--resume` launch, before its frame.
const RESUME_TASK: &str = "fix exactly these";

/// The first message of a fresh launch, before its frame.
const FULL_PROMPT: &str = "do the whole unit";

/// What the fixture agent answers when its environment names no other answer.
const FIXTURE_ANSWER: &str = "the answer";

/// THE STOP's grace in these tests: every silent ending winds down on the end of its input.
const WIND_DOWN_GRACE: Duration = Duration::from_secs(10);

/// The silence bound of a row that goes silent, long enough for a loaded machine to start the
/// fresh launch behind it.
const SILENCE_BOUND: u64 = 3;

/// The progress line of a fallback from [`PRIOR_SESSION`].
const FALLBACK_LINE: &str =
    "stream: session sess-prior to resume yielded no system/init, falling back to a fresh launch";

/// The progress line of the first healthy `system/init` of the fixture agent.
const FRESH_INIT_LINE: &str = "mcp servers: []";

impl Fixture {
    /// The file the fixture agent appends each launch's first message to.
    fn launch_log(&self) -> PathBuf {
        self.scratch_file("launches")
    }

    /// The task of each first message the fixture agent read, oldest first.
    fn tasks_read(&self) -> Vec<String> {
        std::fs::read_to_string(self.launch_log())
            .unwrap_or_default()
            .lines()
            .map(|line| {
                let message: Value = serde_json::from_str(line).unwrap();
                message["message"]["content"].as_str().unwrap().to_string()
            })
            .collect()
    }

    /// Every launch record of the run, `started` zeroed so a record compares whole.
    fn unstamped_records(&self) -> Vec<SpawnLaunched> {
        self.launch_records()
            .into_iter()
            .map(|record| SpawnLaunched {
                started: 0,
                ..record
            })
            .collect()
    }

    /// The session of the latest launch record.
    fn latest_session(&self) -> String {
        self.launch_records()
            .last()
            .expect("a launch record")
            .session_id
            .clone()
    }

    /// The run's transcript files as `(name, lines)`, in name order.
    fn transcripts(&self) -> Vec<(String, usize)> {
        let dir = self.scratch_file("agent-stream").join("run-1");
        let mut transcripts: Vec<(String, usize)> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                (name, std::fs::read_to_string(path).unwrap().lines().count())
            })
            .collect();
        transcripts.sort();
        transcripts
    }

    /// `(output, error, is a liveness fault)` of every `SpawnResult` the run store holds.
    fn recorded_results(&self) -> Vec<(String, String, bool)> {
        common::fixtures::spawn_result_rows(&self.spawn_results())
            .into_iter()
            .map(|(_, output, error, fault)| (output, error, fault))
            .collect()
    }

    /// The environment choosing the fixture agent's two endings and its launch log.
    fn ending_env(&self, resume_ending: &str, fresh_ending: &str) -> Vec<(String, String)> {
        vec![
            (
                "RIGGER_TEST_RESUME_ENDING".to_string(),
                resume_ending.to_string(),
            ),
            (
                "RIGGER_TEST_FRESH_ENDING".to_string(),
                fresh_ending.to_string(),
            ),
            (
                "RIGGER_TEST_LAUNCH_LOG".to_string(),
                self.launch_log().to_string_lossy().into_owned(),
            ),
        ]
    }

    /// [`RESUMED_SPAWN`], fresh, ending as `fresh_ending`, in this fixture's own directory: the
    /// failure proofs pin a stderr tail exactly, and a launch with no directory runs in the
    /// process's, which one test of this binary moves into a directory it then deletes, leaving
    /// the shell a line of its own on that tail.
    fn fresh_opts(&self, fresh_ending: &str) -> SpawnOpts {
        SpawnOpts {
            env: self.ending_env("answer", fresh_ending),
            dir: self.launch_dir(),
            ..opts(RESUMED_SPAWN)
        }
    }

    /// [`RESUMED_SPAWN`] resuming [`PRIOR_SESSION`] with the two endings named.
    fn resumed_opts(&self, resume_ending: &str, fresh_ending: &str) -> SpawnOpts {
        SpawnOpts {
            resumed_from: PRIOR_SESSION.to_string(),
            resume_task: RESUME_TASK.to_string(),
            env: self.ending_env(resume_ending, fresh_ending),
            ..self.fresh_opts(fresh_ending)
        }
    }

    /// Spawn the fixture agent as `o` under `agent`.
    fn host(&self, agent: &AgentDef, o: &SpawnOpts) -> Result<AgentResult, Error> {
        self.stopping_driver(RESUME_ENDING_AGENT, WIND_DOWN_GRACE)
            .spawn(agent, FULL_PROMPT, o, &no_emit)
    }

    /// Record a launch of `spawn` at `launch` on `run`, closed `ended` unless that is empty.
    fn seed_launch(&self, run: &str, spawn: &str, launch: u32, ended: &str) {
        let record = if ended.is_empty() {
            SpawnLaunched {
                spawn: spawn.to_string(),
                launch,
                session_id: "sess-seeded".to_string(),
                ..Default::default()
            }
        } else {
            SpawnLaunched::closed(spawn, launch, "sess-seeded", ended, "")
        };
        rigger::progress_store::record_launch(&self.progress_store, run, &record).unwrap();
    }

    /// The directory every launch of [`fresh_opts`](Self::fresh_opts) runs in: this fixture's own.
    fn launch_dir(&self) -> String {
        self.scratch_root.path().to_string_lossy().into_owned()
    }

    /// The open record of [`RESUMED_SPAWN`]'s launch `launch` as `session`, naming the
    /// [`launch_dir`](Self::launch_dir) it ran in.
    fn opened(&self, launch: u32, session: &str, resumed_from: Option<&str>) -> SpawnLaunched {
        SpawnLaunched {
            spawn: RESUMED_SPAWN.to_string(),
            launch,
            session_id: session.to_string(),
            resumed_from: resumed_from.map(str::to_string),
            dir: self.launch_dir(),
            ..Default::default()
        }
    }
}

/// The closing record of [`RESUMED_SPAWN`]'s launch `launch` as `session`.
fn closed(launch: u32, session: &str, ended: &str, class: &str) -> SpawnLaunched {
    SpawnLaunched::closed(RESUMED_SPAWN, launch, session, ended, class)
}

/// The one result-line a fallback leaves on the run log: the fresh launch's answer.
fn fresh_answer_on_record() -> Vec<(String, String, bool)> {
    vec![(FIXTURE_ANSWER.to_string(), String::new(), false)]
}

/// The liveness fault THE STOP leaves on the run log for a launch silent past [`SILENCE_BOUND`].
fn liveness_fault_on_record() -> Vec<(String, String, bool)> {
    let message = SILENCE_FAILURE.strip_prefix(PORT_PREFIX).unwrap();
    vec![(String::new(), message.to_string(), true)]
}

/// The whole of one fallback: `result` is the fresh launch's, the `--resume` launch closed
/// `interrupted` and the fresh one opened at the next ordinal and closed `completed`, the tasks in
/// `tasks` were read, the only result on the run log is the fresh answer, so no attempt was
/// charged, the progress log holds `noted` then the fallback's line, and the refused launch's
/// transcript holds `refused_lines` lines beside the fresh launch's two.
fn assert_fell_back(
    fx: &Fixture,
    result: &AgentResult,
    tasks: &[String],
    noted: &[&str],
    refused_lines: usize,
) {
    let fresh = fx.latest_session();
    assert_ne!(fresh, PRIOR_SESSION);
    assert_eq!(
        fx.unstamped_records(),
        vec![
            fx.opened(0, PRIOR_SESSION, Some(PRIOR_SESSION)),
            closed(0, PRIOR_SESSION, "interrupted", ""),
            fx.opened(1, &fresh, None),
            closed(1, &fresh, "completed", ""),
        ]
    );
    assert_eq!(result.output, FIXTURE_ANSWER);
    assert_eq!(result.session_id, fresh);
    assert_eq!(result.resumed_from, "");
    assert_eq!(fx.tasks_read(), tasks);
    assert_eq!(fx.recorded_results(), fresh_answer_on_record());
    let lines: Vec<&str> = noted
        .iter()
        .copied()
        .chain([FALLBACK_LINE, FRESH_INIT_LINE])
        .collect();
    assert_eq!(fx.activities(), lines);
    assert_eq!(
        fx.transcripts(),
        vec![
            (format!("{RESUMED_STEM}.0.jsonl"), refused_lines),
            (format!("{RESUMED_STEM}.1.jsonl"), 2),
        ]
    );
}

/// The longest one spawn call of a fallback row takes, well under the 30 s an `unreadable` ending
/// stays alive past its line: a host that waited such a session out never ended the read.
const FALLBACK_WITHIN: Duration = Duration::from_secs(20);

/// THE RESUME FALLBACK: a `--resume` launch of Claude Code ending `ending` under `agent` yields
/// no `system/init`, so one spawn call lands the fresh launch behind it ([`assert_fell_back`]),
/// within [`FALLBACK_WITHIN`].
fn a_resume_ending_with_no_session_falls_back(
    ending: &str,
    agent: AgentDef,
    noted: &[&str],
    refused_lines: usize,
) {
    let fx = Fixture::new();
    let o = fx.resumed_opts(ending, "answer");

    let started = Instant::now();
    let result = fx.host(&agent, &o).expect("the fresh launch answers");
    let elapsed = started.elapsed();

    assert!(
        elapsed < FALLBACK_WITHIN,
        "the resume ends its read at the ending, never waiting its session out: {elapsed:?}"
    );
    assert_fell_back(
        &fx,
        &result,
        &[framed(RESUME_TASK), framed(FULL_PROMPT)],
        noted,
        refused_lines,
    );
}

rigger::test_cases! {
    a_resume_refused_at_a_clean_end_of_stream_falls_back:
        a_resume_ending_with_no_session_falls_back("refused-eof", AgentDef::default(), &[], 0);
    // Claude Code 2.1.314 refuses with a `result` line and no `system/init`; recording it would
    // stand in the fresh launch's place on the run log.
    a_resume_refused_with_a_result_line_falls_back:
        a_resume_ending_with_no_session_falls_back("refused-result", AgentDef::default(), &[], 1);
    // The refusal's result closes the input, so a session waiting on it can end.
    a_resume_refused_with_a_result_line_and_its_input_held_open_falls_back:
        a_resume_ending_with_no_session_falls_back(
            "refused-result-held-open",
            AgentDef::default(),
            &[],
            1,
        );
    a_resume_silent_to_the_stop_falls_back:
        a_resume_ending_with_no_session_falls_back("silent", wall_clock_bounded(SILENCE_BOUND), &[], 0);
    a_resume_ending_on_an_unreadable_line_falls_back:
        a_resume_ending_with_no_session_falls_back(
            "unreadable",
            AgentDef::default(),
            &["stream (read error before system/init): stream did not contain valid UTF-8"],
            0,
        );
    // A success `result` with no `system/init` before it is no session's answer either.
    a_resume_answering_with_no_init_falls_back:
        a_resume_ending_with_no_session_falls_back("answer-no-init", AgentDef::default(), &[], 1);
}

/// A session gone before it reads a task too big for the pipe breaks the host's write, and it
/// has yielded no `system/init` all the same.
#[test]
fn a_resume_refused_before_it_reads_its_task_falls_back() {
    let fx = Fixture::new();
    let o = SpawnOpts {
        resume_task: "x".repeat(1024 * 1024),
        ..fx.resumed_opts("refused-unread", "answer")
    };

    let result = fx
        .host(&AgentDef::default(), &o)
        .expect("the fresh launch answers");

    assert_fell_back(&fx, &result, &[framed(FULL_PROMPT)], &[], 0);
}

/// What a launch leaves, read by [`a_launch_ends_as_the_hold_and_its_class_decide`].
#[derive(Default)]
enum Ending {
    /// It closes `interrupted` under the class and returns the failure, its scratch kept.
    Interrupted(&'static str, String),
    /// It closes `fault` under the class and returns the failure, its scratch reclaimed.
    Faulted(&'static str, String),
    /// It takes the fallback: its classless `interrupted` close, then the fresh answer.
    #[default]
    FellBack,
    /// It is stopped silent past [`SILENCE_BOUND`], keeping its `stopped` close.
    Stopped,
    /// It fails on an unreadable line, which closes nothing.
    Unreadable,
}

/// One launch of the fixture agent as [`RESUMED_SPAWN`]: how it ends and the world it ends in, then
/// what it leaves.
#[derive(Default)]
struct HeldEnding {
    name: &'static str,
    /// Whether the launch carries no `--resume`.
    fresh: bool,
    /// The fixture agent's ending of the launch.
    ending: &'static str,
    /// The category a `retried-*` ending prints.
    category: &'static str,
    /// What stands on the run stream behind its `RunStarted` before the launch.
    seeded: Vec<Event>,
    /// Whether the host is built with no hold sink.
    no_sink: bool,
    agent: AgentDef,
    /// Whether a hold lands while the launch ends, past the point the fixture agent's `stuck` or
    /// `unreadable-pipes-held` ending announces, with the launch not yet over.
    lands: bool,
    /// The `RunHeld`s the spawn's time appends, in order, a hold that lands among them.
    holds: Vec<RunHeld>,
    left: Ending,
}

/// [`NO_RESULT_FAILURE`] under `class`.
fn classed_failure(class: &str) -> String {
    NO_RESULT_FAILURE.replace("class unknown", &format!("class {class}"))
}

/// Every ending the host weighs against the hold, on a resume that never showed its session unless
/// the row says otherwise.
fn held_ending_cases() -> Vec<HeldEnding> {
    let login = "authentication_failed";
    let held = || vec![operator_hold()];
    let released = || {
        vec![
            operator_hold(),
            common::fixtures::released_event("operator", 0),
        ]
    };
    let bounded = || wall_clock_bounded(SILENCE_BOUND);
    vec![
        HeldEnding {
            name: "a wait class on an unheld run",
            ending: "retried-eof",
            category: "rate_limit",
            holds: vec![hold("rate_limit", "", RETRIES_BY_ITSELF, false, "")],
            left: Ending::Interrupted("rate_limit", classed_failure("rate_limit")),
            ..HeldEnding::default()
        },
        HeldEnding {
            name: "a hold class on an unheld run",
            ending: "retried-eof",
            category: login,
            holds: vec![login_hold()],
            left: Ending::Interrupted(login, classed_failure(login)),
            ..HeldEnding::default()
        },
        HeldEnding {
            name: "a fault class on a held run",
            ending: "retried-eof",
            category: "invalid_request",
            seeded: held(),
            left: Ending::Interrupted("invalid_request", classed_failure("invalid_request")),
            ..HeldEnding::default()
        },
        HeldEnding {
            name: "a refusal at the end of the stream on a held run",
            ending: "refused-eof",
            seeded: held(),
            left: Ending::Interrupted("unknown", REFUSED_FAILURE.to_string()),
            ..HeldEnding::default()
        },
        HeldEnding {
            name: "a refusal with a result line on a held run",
            ending: "refused-result",
            seeded: held(),
            left: Ending::Interrupted("unknown", REFUSED_FAILURE.to_string()),
            ..HeldEnding::default()
        },
        HeldEnding {
            name: "a resume that showed its session, on a held run",
            ending: "init-eof",
            seeded: held(),
            left: Ending::Interrupted("unknown", NO_RESULT_FAILURE.to_string()),
            ..HeldEnding::default()
        },
        HeldEnding {
            name: "silence on a held run",
            ending: "silent",
            seeded: held(),
            agent: bounded(),
            left: Ending::Stopped,
            ..HeldEnding::default()
        },
        HeldEnding {
            name: "silence on the hold the launch's own retry sighted",
            ending: "retried-silent",
            category: login,
            agent: bounded(),
            holds: vec![login_hold()],
            left: Ending::Stopped,
            ..HeldEnding::default()
        },
        HeldEnding {
            name: "an unreadable line on a held run",
            ending: "unreadable",
            seeded: held(),
            left: Ending::Unreadable,
            ..HeldEnding::default()
        },
        HeldEnding {
            name: "an unreadable line on the hold the launch's own retry sighted",
            ending: "retried-unreadable",
            category: login,
            holds: vec![login_hold()],
            left: Ending::Unreadable,
            ..HeldEnding::default()
        },
        HeldEnding {
            name: "a hold landing inside the stop grace",
            ending: "stuck",
            agent: bounded(),
            lands: true,
            holds: vec![RunHeld::default()],
            left: Ending::Stopped,
            ..HeldEnding::default()
        },
        HeldEnding {
            name: "a hold landing between a read error and the end of the drain joins",
            ending: "unreadable-pipes-held",
            lands: true,
            holds: vec![RunHeld::default()],
            left: Ending::Unreadable,
            ..HeldEnding::default()
        },
        HeldEnding {
            name: "a fault class on an unheld run",
            ending: "retried-eof",
            category: "invalid_request",
            ..HeldEnding::default()
        },
        HeldEnding {
            name: "a refusal on an unheld run",
            ending: "refused-eof",
            ..HeldEnding::default()
        },
        HeldEnding {
            name: "a refusal on a released hold",
            ending: "refused-eof",
            seeded: released(),
            ..HeldEnding::default()
        },
        HeldEnding {
            name: "a wait class on a host with no sink",
            ending: "retried-eof",
            category: "rate_limit",
            no_sink: true,
            ..HeldEnding::default()
        },
        HeldEnding {
            name: "a held run on a host with no sink",
            ending: "refused-eof",
            seeded: held(),
            no_sink: true,
            ..HeldEnding::default()
        },
        HeldEnding {
            name: "a wait class silent on an unheld run",
            ending: "retried-silent",
            category: "rate_limit",
            agent: bounded(),
            ..HeldEnding::default()
        },
        HeldEnding {
            name: "a wait class unreadable on an unheld run",
            ending: "retried-unreadable",
            category: "rate_limit",
            ..HeldEnding::default()
        },
        HeldEnding {
            name: "a launch fault on a held run",
            fresh: true,
            ending: "init-server-failed",
            seeded: held(),
            left: Ending::Interrupted("unknown", SERVER_FAILURE.to_string()),
            ..HeldEnding::default()
        },
        HeldEnding {
            name: "a launch fault on an unheld run",
            fresh: true,
            ending: "init-server-failed",
            left: Ending::Faulted("unknown", SERVER_FAILURE.to_string()),
            ..HeldEnding::default()
        },
        HeldEnding {
            name: "a launch fault on a held run, on a host with no sink",
            fresh: true,
            ending: "init-server-failed",
            seeded: held(),
            no_sink: true,
            left: Ending::Faulted("unknown", SERVER_FAILURE.to_string()),
            ..HeldEnding::default()
        },
    ]
}

/// HELD MEANS NO LAUNCH AND NO CHARGE at the host: an ending of a class that holds or waits, or any
/// `fault` ending on a held run, closes `interrupted`, records no result and keeps its scratch, a
/// `--resume` launch with no `system/init` among them launching nothing fresh. On a run held at the
/// launch's end the stop and the read error keep their endings and launch nothing fresh, a hold
/// that lands while the launch ends among them, and on an unheld run they and a fault class take
/// the fallback. A host with no sink reads no hold and keeps `fault` and the fallback.
#[cfg(unix)]
#[test]
fn a_launch_ends_as_the_hold_and_its_class_decide() {
    for case in held_ending_cases() {
        let name = case.name;
        let fx = Fixture::started(&case.seeded);
        let before = fx.holds().len();
        let driver = Driver {
            hold_sink: (!case.no_sink).then_some(&fx.run_store as &dyn EventStore),
            ..fx.stopping_driver(RESUME_ENDING_AGENT, WIND_DOWN_GRACE)
        };
        let mut o = if case.fresh {
            fx.fresh_opts(case.ending)
        } else {
            fx.resumed_opts(case.ending, "answer")
        };
        o.env.push((
            "RIGGER_TEST_RETRY_CATEGORY".to_string(),
            case.category.to_string(),
        ));
        let seen = fx.scratch_file("ending-seen");
        let release = fx.scratch_file("ending-release");
        o.env.extend([
            (
                "RIGGER_TEST_ENDING_SEEN_FILE".to_string(),
                seen.to_string_lossy().into_owned(),
            ),
            (
                "RIGGER_TEST_ENDING_RELEASE_FILE".to_string(),
                release.to_string_lossy().into_owned(),
            ),
        ]);
        let planted = fx.plant_scratch(&o);
        let (closes, failure, launches, results, kept) = match case.left {
            Ending::Interrupted(class, failure) => (
                vec![(0, "interrupted", Some(class))],
                failure,
                1,
                Vec::new(),
                true,
            ),
            Ending::Faulted(class, failure) => (
                vec![(0, "fault", Some(class))],
                failure,
                1,
                Vec::new(),
                false,
            ),
            Ending::FellBack => (
                vec![(0, "interrupted", None), (1, "completed", None)],
                String::new(),
                2,
                fresh_answer_on_record(),
                false,
            ),
            Ending::Stopped => (
                vec![(0, "stopped", None)],
                SILENCE_FAILURE.to_string(),
                1,
                liveness_fault_on_record(),
                false,
            ),
            Ending::Unreadable => (
                Vec::new(),
                UNREADABLE_FAILURE.to_string(),
                1,
                Vec::new(),
                false,
            ),
        };

        let outcome = if case.lands {
            let (ending_seen, outcome) = std::thread::scope(|scope| {
                let session = scope.spawn(|| driver.spawn(&case.agent, FULL_PROMPT, &o, &no_emit));
                let ending_seen = wait_until_for(800, || seen.exists());
                fx.hold_the_run();
                std::fs::write(&release, "").unwrap();
                (ending_seen, session.join().unwrap())
            });
            assert!(
                ending_seen,
                "{name}: the ending is under way when the hold lands"
            );
            outcome
        } else {
            driver.spawn(&case.agent, FULL_PROMPT, &o, &no_emit)
        };

        let spawned = match outcome {
            Ok(result) => {
                assert_eq!(result.output, FIXTURE_ANSWER, "{name}: the answer");
                String::new()
            }
            Err(e) => rigger::conductor::strip_failure_marker(&e).to_string(),
        };
        assert_eq!(spawned, failure, "{name}: the failure");
        let records = fx.launch_records();
        let closed: Vec<_> = records
            .iter()
            .filter_map(|r| Some((r.launch, r.ended.as_deref()?, r.class.as_deref())))
            .collect();
        assert_eq!(closed, closes, "{name}: the closes");
        assert_eq!(fx.tasks_read().len(), launches, "{name}: the launches");
        assert_eq!(fx.holds()[before..], case.holds, "{name}: the holds");
        assert_eq!(fx.recorded_results(), results, "{name}: the results");
        assert_eq!(planted.exists(), kept, "{name}: the scratch");
    }
}

/// A wait-class ending is answered by its class alone: on a sink that reads nothing it reads no
/// hold and counts no close, closing `interrupted` with its scratch kept and its lost sighting on
/// its progress.
#[cfg(unix)]
#[test]
fn a_wait_class_ending_reads_no_hold_and_counts_no_close() {
    let fx = Fixture::started(&[]);
    let armed = std::sync::atomic::AtomicBool::new(true);
    let sink = common::fixtures::ReadsFailWhenArmed {
        inner: &fx.run_store,
        armed: &armed,
    };
    let driver = Driver {
        hold_sink: Some(&sink),
        ..fx.stopping_driver(RESUME_ENDING_AGENT, WIND_DOWN_GRACE)
    };
    let mut o = fx.fresh_opts("retried-eof");
    o.env.push((
        "RIGGER_TEST_RETRY_CATEGORY".to_string(),
        "rate_limit".to_string(),
    ));
    let planted = fx.plant_scratch(&o);

    let failure = driver
        .spawn(&AgentDef::default(), FULL_PROMPT, &o, &no_emit)
        .map(|_| ())
        .unwrap_err();

    assert_eq!(
        rigger::conductor::strip_failure_marker(&failure),
        classed_failure("rate_limit")
    );
    let closing = fx.launch_records().pop().unwrap();
    assert_eq!(
        (closing.ended.as_deref(), closing.class.as_deref()),
        (Some("interrupted"), Some("rate_limit"))
    );
    assert_eq!(
        fx.activities(),
        vec![
            "waiting: rate_limit (attempt 1, retrying in 1ms)",
            "hold sighting rate_limit not recorded: event store: the run read failed",
        ]
    );
    assert!(planted.exists(), "the scratch is kept");
}

/// A hold the host cannot read proves the run neither held nor unheld. A run not proved unheld
/// launches nothing and a failed read masks no failure, so with a line on its progress each ending
/// stands as the launch's own: no fallback, no `interrupted` close it did not prove, every close
/// unstamped and uncounted.
#[cfg(unix)]
#[test]
fn a_hold_read_that_fails_launches_nothing_fresh_and_masks_no_failure() {
    struct Row<'a> {
        name: &'static str,
        resumed: bool,
        ending: &'static str,
        agent: AgentDef,
        closes: Vec<(u32, &'static str, Option<&'static str>)>,
        failure: &'static str,
        results: Vec<(String, String, bool)>,
        activities: Vec<&'a str>,
    }
    let unread = "hold not read: event store: the run read failed";
    let uncounted = "failed closes not counted: event store: the run read failed";
    let stopping = format!(
        "stream: {}",
        SILENCE_FAILURE.strip_prefix(PORT_PREFIX).unwrap()
    );
    let rows = [
        Row {
            name: "a fault ending",
            resumed: false,
            ending: "init-eof",
            agent: AgentDef::default(),
            closes: vec![(0, "fault", Some("unknown"))],
            failure: NO_RESULT_FAILURE,
            results: Vec::new(),
            activities: vec![FRESH_INIT_LINE, unread, uncounted],
        },
        Row {
            name: "a resume with no session",
            resumed: true,
            ending: "refused-eof",
            agent: AgentDef::default(),
            closes: vec![(0, "fault", Some("unknown"))],
            failure: REFUSED_FAILURE,
            results: Vec::new(),
            activities: vec![unread, uncounted],
        },
        Row {
            name: "a resume with no session, silent to the stop",
            resumed: true,
            ending: "silent",
            agent: wall_clock_bounded(SILENCE_BOUND),
            closes: vec![(0, "stopped", None)],
            failure: SILENCE_FAILURE,
            results: liveness_fault_on_record(),
            activities: vec![unread, &stopping, uncounted],
        },
        Row {
            name: "a resume with no session, on a read error",
            resumed: true,
            ending: "unreadable",
            agent: AgentDef::default(),
            closes: Vec::new(),
            failure: UNREADABLE_FAILURE,
            results: Vec::new(),
            activities: vec![
                "stream (read error before system/init): stream did not contain valid UTF-8",
                unread,
            ],
        },
    ];
    for row in rows {
        let name = row.name;
        let fx = Fixture::started(&[operator_hold()]);
        let armed = std::sync::atomic::AtomicBool::new(true);
        let sink = common::fixtures::ReadsFailWhenArmed {
            inner: &fx.run_store,
            armed: &armed,
        };
        let driver = Driver {
            hold_sink: Some(&sink),
            ..fx.stopping_driver(RESUME_ENDING_AGENT, WIND_DOWN_GRACE)
        };
        let o = if row.resumed {
            fx.resumed_opts(row.ending, "answer")
        } else {
            fx.fresh_opts(row.ending)
        };
        let planted = fx.plant_scratch(&o);

        let outcome = driver.spawn(&row.agent, FULL_PROMPT, &o, &no_emit);

        let failure = outcome.map_or_else(
            |e| rigger::conductor::strip_failure_marker(&e).to_string(),
            |_| String::new(),
        );
        assert_eq!(failure, row.failure, "{name}: the failure");
        let records = fx.launch_records();
        let closes: Vec<_> = records
            .iter()
            .filter_map(|r| Some((r.launch, r.ended.as_deref()?, r.class.as_deref())))
            .collect();
        assert_eq!(closes, row.closes, "{name}: the closes");
        assert_eq!(fx.tasks_read().len(), 1, "{name}: the one launch");
        assert_eq!(fx.recorded_results(), row.results, "{name}: the results");
        assert_eq!(fx.activities(), row.activities, "{name}: the progress");
        assert!(
            records.iter().all(|r| r.revision.is_none()),
            "{name}: no close is stamped: {records:?}"
        );
        assert!(!planted.exists(), "{name}: the scratch is reclaimed");
    }
}

/// The one launch of `o`, its outcome `outcome`, stands: no fallback ran. It opened naming
/// `resumed_from`, closed `ended` when that names a closing, and failed with `failure` unless
/// that is empty, leaving `results` on the run log.
fn assert_one_launch_stands(
    fx: &Fixture,
    outcome: Result<AgentResult, Error>,
    resumed_from: Option<&str>,
    ended: Option<(&str, &str)>,
    failure: &str,
    results: Vec<(String, String, bool)>,
) {
    let session = fx.latest_session();
    let mut records = vec![fx.opened(0, &session, resumed_from)];
    records.extend(ended.map(|(ended, class)| closed(0, &session, ended, class)));
    assert_eq!(fx.unstamped_records(), records);
    assert_eq!(
        outcome
            .map(|result| (result.session_id, result.resumed_from))
            .map_err(|e| rigger::conductor::strip_failure_marker(&e).to_string()),
        if failure.is_empty() {
            Ok((session, resumed_from.unwrap_or_default().to_string()))
        } else {
            Err(failure.to_string())
        }
    );
    assert_eq!(fx.recorded_results(), results);
    assert_eq!(
        fx.transcripts()
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>(),
        vec![format!("{RESUMED_STEM}.0.jsonl")]
    );
}

/// A launch that saw `system/init` takes no fallback, whatever its ending.
fn a_resume_that_saw_its_session_stands(
    ending: &str,
    agent: AgentDef,
    ended: Option<(&str, &str)>,
    failure: &str,
    results: Vec<(String, String, bool)>,
) {
    let fx = Fixture::new();
    let o = fx.resumed_opts(ending, "answer");
    let outcome = fx.host(&agent, &o);
    assert_one_launch_stands(&fx, outcome, Some(PRIOR_SESSION), ended, failure, results);
}

/// A fresh launch names no session to lose, so nothing it does takes the fallback.
fn a_fresh_launch_stands(
    ending: &str,
    agent: AgentDef,
    ended: Option<(&str, &str)>,
    failure: &str,
    results: Vec<(String, String, bool)>,
) {
    let fx = Fixture::new();
    let o = fx.fresh_opts(ending);
    let outcome = fx.host(&agent, &o);
    assert_one_launch_stands(&fx, outcome, None, ended, failure, results);
}

rigger::test_cases! {
    a_resume_answering_stands: a_resume_that_saw_its_session_stands(
        "answer",
        AgentDef::default(),
        Some(("completed", "")),
        "",
        fresh_answer_on_record(),
    );
    a_resume_ending_after_init_stands_as_a_fault: a_resume_that_saw_its_session_stands(
        "init-eof",
        AgentDef::default(),
        Some(("fault", "unknown")),
        NO_RESULT_FAILURE,
        Vec::new(),
    );
    a_resume_silent_after_init_stands_stopped: a_resume_that_saw_its_session_stands(
        "init-silent",
        wall_clock_bounded(SILENCE_BOUND),
        Some(("stopped", "")),
        SILENCE_FAILURE,
        liveness_fault_on_record(),
    );
    a_resume_unreadable_after_init_stands: a_resume_that_saw_its_session_stands(
        "init-unreadable",
        AgentDef::default(),
        None,
        UNREADABLE_FAILURE,
        Vec::new(),
    );
    a_resume_whose_server_failed_at_init_stands: a_resume_that_saw_its_session_stands(
        "init-server-failed",
        AgentDef::default(),
        Some(("fault", "unknown")),
        SERVER_FAILURE,
        Vec::new(),
    );
    a_fresh_launch_ending_with_nothing_stands_as_a_fault: a_fresh_launch_stands(
        "refused-eof",
        AgentDef::default(),
        Some(("fault", "unknown")),
        REFUSED_FAILURE,
        Vec::new(),
    );
    a_fresh_launch_silent_stands_stopped: a_fresh_launch_stands(
        "silent",
        wall_clock_bounded(SILENCE_BOUND),
        Some(("stopped", "")),
        SILENCE_FAILURE,
        liveness_fault_on_record(),
    );
    a_fresh_launch_unreadable_stands: a_fresh_launch_stands(
        "unreadable",
        AgentDef::default(),
        None,
        UNREADABLE_FAILURE,
        Vec::new(),
    );
    // Only a launch that asked to resume reads a missing init as a session gone.
    a_fresh_launch_answering_with_no_init_stands: a_fresh_launch_stands(
        "answer-no-init",
        AgentDef::default(),
        Some(("completed", "")),
        "",
        fresh_answer_on_record(),
    );
}

/// The fresh launch a fallback lands is the last of the spawn call: ending `fresh_ending`, it
/// closes `ended` at its own ordinal, fails with `failure` and no third launch follows.
fn a_fallback_launch_ends_at_its_own_ordinal(
    resume_ending: &str,
    fresh_ending: &str,
    agent: AgentDef,
    ended: Option<(&str, &str)>,
    failure: &str,
    results: Vec<(String, String, bool)>,
) {
    let fx = Fixture::new();
    let o = fx.resumed_opts(resume_ending, fresh_ending);

    let err = fx
        .host(&agent, &o)
        .expect_err("the fresh launch's ending stands");

    let fresh = fx.latest_session();
    let mut records = vec![
        fx.opened(0, PRIOR_SESSION, Some(PRIOR_SESSION)),
        closed(0, PRIOR_SESSION, "interrupted", ""),
        fx.opened(1, &fresh, None),
    ];
    records.extend(ended.map(|(ended, class)| closed(1, &fresh, ended, class)));
    assert_eq!(fx.unstamped_records(), records);
    assert_eq!(rigger::conductor::strip_failure_marker(&err), failure);
    assert_eq!(fx.recorded_results(), results);
    assert_eq!(fx.tasks_read().len(), 2);
}

rigger::test_cases! {
    a_fallback_launch_ending_with_nothing_is_a_fault: a_fallback_launch_ends_at_its_own_ordinal(
        "refused-eof",
        "refused-eof",
        AgentDef::default(),
        Some(("fault", "unknown")),
        REFUSED_FAILURE,
        Vec::new(),
    );
    a_fallback_launch_silent_is_stopped: a_fallback_launch_ends_at_its_own_ordinal(
        "refused-eof",
        "silent",
        wall_clock_bounded(SILENCE_BOUND),
        Some(("stopped", "")),
        SILENCE_FAILURE,
        liveness_fault_on_record(),
    );
    a_fallback_launch_whose_server_failed_is_a_fault: a_fallback_launch_ends_at_its_own_ordinal(
        "refused-eof",
        "init-server-failed",
        AgentDef::default(),
        Some(("fault", "unknown")),
        SERVER_FAILURE,
        Vec::new(),
    );
    a_fallback_launch_unreadable_fails_open: a_fallback_launch_ends_at_its_own_ordinal(
        "unreadable",
        "unreadable",
        AgentDef::default(),
        None,
        UNREADABLE_FAILURE,
        Vec::new(),
    );
}

/// A launch asked to take `launch` opens and closes at the ordinal after the highest the progress
/// store records for its spawn on its run, a closing counting as much as an opening, and at
/// `launch` when none does: `seeded` rows are `(run, spawn, ordinal, ended)`.
fn the_ordinal_follows_the_record(seeded: &[(&str, &str, u32, &str)], launch: u32, ordinal: u32) {
    let fx = Fixture::new();
    for (run, spawn, recorded, ended) in seeded {
        fx.seed_launch(run, spawn, *recorded, ended);
    }
    let o = SpawnOpts {
        launch,
        ..fx.fresh_opts("answer")
    };

    fx.host(&AgentDef::default(), &o)
        .expect("the launch answers");

    let session = fx.latest_session();
    let records = fx.unstamped_records();
    assert_eq!(
        records[records.len() - 2..],
        [
            fx.opened(ordinal, &session, None),
            closed(ordinal, &session, "completed", ""),
        ]
    );
    assert_eq!(
        fx.transcripts(),
        vec![(format!("{RESUMED_STEM}.{ordinal}.jsonl"), 2)]
    );
}

rigger::test_cases! {
    a_first_launch_takes_ordinal_zero: the_ordinal_follows_the_record(&[], 0, 0);
    a_first_launch_keeps_the_ordinal_its_spawn_names: the_ordinal_follows_the_record(&[], 2, 2);
    a_recorded_launch_moves_the_next_to_one:
        the_ordinal_follows_the_record(&[("run-1", RESUMED_SPAWN, 0, "")], 0, 1);
    a_recorded_launch_outranks_the_ordinal_a_spawn_names:
        the_ordinal_follows_the_record(&[("run-1", RESUMED_SPAWN, 0, "")], 9, 1);
    a_closing_record_counts_once_with_its_opening: the_ordinal_follows_the_record(
        &[
            ("run-1", RESUMED_SPAWN, 0, ""),
            ("run-1", RESUMED_SPAWN, 0, "fault"),
        ],
        0,
        1,
    );
    a_closing_record_alone_counts:
        the_ordinal_follows_the_record(&[("run-1", RESUMED_SPAWN, 4, "stopped")], 0, 5);
    the_highest_recorded_ordinal_decides_not_the_latest: the_ordinal_follows_the_record(
        &[
            ("run-1", RESUMED_SPAWN, 0, ""),
            ("run-1", RESUMED_SPAWN, 3, ""),
            ("run-1", RESUMED_SPAWN, 1, ""),
        ],
        0,
        4,
    );
    another_spawns_launches_move_no_ordinal:
        the_ordinal_follows_the_record(&[("run-1", "u105-resume/implementer#0", 7, "")], 0, 0);
    another_runs_launches_move_no_ordinal:
        the_ordinal_follows_the_record(&[("run-2", RESUMED_SPAWN, 7, "")], 0, 0);
}

/// Every close carries the ordinal its launch took, past `opts.launch`: ending `ending`, the
/// launch after a recorded launch 4 closes `ended` at 5.
fn a_close_carries_the_ordinal_its_launch_took(ending: &str, agent: AgentDef, ended: (&str, &str)) {
    let fx = Fixture::new();
    fx.seed_launch("run-1", RESUMED_SPAWN, 4, "");

    fx.host(&agent, &fx.fresh_opts(ending))
        .expect_err("the launch fails");

    assert_eq!(
        fx.unstamped_records().last(),
        Some(&closed(5, &fx.latest_session(), ended.0, ended.1))
    );
}

rigger::test_cases! {
    a_fault_closes_at_the_launchs_ordinal:
        a_close_carries_the_ordinal_its_launch_took(
            "init-eof",
            AgentDef::default(),
            ("fault", "unknown"),
        );
    a_launch_fault_closes_at_the_launchs_ordinal:
        a_close_carries_the_ordinal_its_launch_took(
            "init-server-failed",
            AgentDef::default(),
            ("fault", "unknown"),
        );
    a_stop_closes_at_the_launchs_ordinal:
        a_close_carries_the_ordinal_its_launch_took(
            "silent",
            wall_clock_bounded(SILENCE_BOUND),
            ("stopped", ""),
        );
}

/// The fallback's two launches take the two ordinals after the record, past `opts.launch`, and
/// neither touches a transcript of the prior launches'.
#[test]
fn a_fallback_past_recorded_launches_takes_the_next_two_ordinals() {
    let fx = Fixture::new();
    fx.seed_launch("run-1", RESUMED_SPAWN, 4, "");
    let o = fx.resumed_opts("refused-result", "answer");

    fx.host(&AgentDef::default(), &o)
        .expect("the fresh launch answers");

    let fresh = fx.latest_session();
    assert_eq!(
        fx.unstamped_records()[1..].to_vec(),
        vec![
            fx.opened(5, PRIOR_SESSION, Some(PRIOR_SESSION)),
            closed(5, PRIOR_SESSION, "interrupted", ""),
            fx.opened(6, &fresh, None),
            closed(6, &fresh, "completed", ""),
        ]
    );
    assert_eq!(
        fx.transcripts(),
        vec![
            (format!("{RESUMED_STEM}.5.jsonl"), 1),
            (format!("{RESUMED_STEM}.6.jsonl"), 2),
        ]
    );
}

/// A restart spawns an id an earlier launch of its run left unended with the spawn's own `launch`
/// at 0: the transcript of the first launch survives it.
#[test]
fn a_restarts_first_spawn_keeps_the_prior_transcript() {
    let fx = Fixture::new();
    let ending = |fresh_ending: &str| SpawnOpts {
        env: fx.ending_env("answer", fresh_ending),
        ..opts(RESUMED_SPAWN)
    };

    fx.host(&AgentDef::default(), &ending("init-eof"))
        .expect_err("a launch ending with no result leaves its spawn unended");
    fx.host(&AgentDef::default(), &ending("answer"))
        .expect("the restart's launch answers");

    assert_eq!(
        fx.transcripts(),
        vec![
            (format!("{RESUMED_STEM}.0.jsonl"), 1),
            (format!("{RESUMED_STEM}.1.jsonl"), 2),
        ]
    );
}

/// The session of the launch a [`relaunch_cases`] row interrupts, fresh unless the row says.
const INTERRUPTED: &str = "sess-interrupted";

/// The session of the fresh launch a fallback landed before a [`relaunch_cases`] row.
const FELL_BACK: &str = "sess-fell-back";

/// The session a relaunch before a [`relaunch_cases`] row forked its interrupted session under.
const RELAUNCHED: &str = "sess-relaunched";

/// Stands, in a [`relaunch_cases`] row, for the session the host mints for a fresh launch.
const MINTED: &str = "<the session the host mints>";

/// Stands, in a [`relaunch_cases`] row, for the session the host mints for the fork its
/// relaunch resumes the interrupted session as.
const FORKED: &str = "<the session the host forks>";

/// The note of a relaunch after a supervisor restart.
const RESTARTED: &str =
    "This session was interrupted: rigger's supervisor restarted. Continue where you left off.";

/// The note of a relaunch after [`login_hold`] stood 42s and was released.
const LOGIN_RELEASED: &str = "This session was interrupted: the run was held 42s \
    (authentication_failed - run claude auth login) and is released now. Continue where you left \
    off.";

/// The note of a relaunch after an `authentication_failed` close no release is known to follow.
const LOGIN_CLASS: &str =
    "This session was interrupted by authentication_failed. Continue where you left off.";

/// What a [`relaunch_cases`] row's undecodable run-stream event carries.
const UNDECODABLE: &[u8] = b"not json";

/// A launch record of [`RESUMED_SPAWN`] a [`relaunch_cases`] row states.
#[derive(Clone, Copy)]
enum Record {
    /// Launch `.0` opened as session `.1` resuming `.2`, in the fixture's launch directory.
    Opened(u32, &'static str, Option<&'static str>),
    /// Launch `.0` of session `.1` closed `.2` under class `.3`, stamped with the revision of the
    /// run-log event at index `.4`.
    Closed(u32, &'static str, &'static str, &'static str, Option<usize>),
}

/// The hold sink a [`relaunch_cases`] row builds its host with.
#[derive(Default)]
enum Sink {
    /// The fixture's run store.
    #[default]
    Run,
    /// The fixture's run store failing every read.
    Failing,
    /// None.
    Absent,
}

/// One spawn of [`RESUMED_SPAWN`] behind the launches a row records: its world, then what it
/// leaves.
#[derive(Default)]
struct Relaunch {
    name: &'static str,
    /// What stands on the run stream behind its `RunStarted` before the spawn.
    log: Vec<Event>,
    /// The launch records standing before the spawn.
    seeded: Vec<Record>,
    /// Whether the seeded records are a prior run's.
    prior_run: bool,
    /// Whether the spawn resumes [`PRIOR_SESSION`] with [`RESUME_TASK`].
    resumed: bool,
    /// The fixture agent's ending of a `--resume` launch, empty answering.
    resume_ending: &'static str,
    sink: Sink,
    /// The records the spawn appends, their stamps not compared.
    appended: Vec<Record>,
    /// The first message of each launch, before its frame.
    tasks: Vec<String>,
    /// `(session_id, resumed_from)` of the answer, unless `failure` names the spawn's failure.
    answered: (&'static str, &'static str),
    failure: String,
}

impl Fixture {
    /// `record` as the progress store holds it, [`FORKED`] read as `forked` and [`MINTED`] as
    /// `minted`.
    fn record(&self, record: Record, forked: &str, minted: &str) -> SpawnLaunched {
        let session = |session: &'static str| match session {
            FORKED => forked,
            MINTED => minted,
            named => named,
        };
        match record {
            Record::Opened(launch, id, resumed_from) => {
                self.opened(launch, session(id), resumed_from)
            }
            Record::Closed(launch, id, ended, class, stamp) => SpawnLaunched {
                revision: stamp.map(|at| self.run_log()[at].revision),
                ..closed(launch, session(id), ended, class)
            },
        }
    }
}

/// Every spawn the relaunch table drives. The run log every stamp indexes holds the `RunStarted`,
/// then the row's `log`.
fn relaunch_cases() -> Vec<Relaunch> {
    use common::fixtures::{released_event, run_held_event};
    use Record::{Closed, Opened};
    let released = |held_for_s| released_event(rigger::run::HOLD_BY_OPERATOR, held_for_s);
    let login_released = || vec![run_held_event(&login_hold()), released(42)];
    let login_close = |stamp| {
        vec![
            Opened(0, INTERRUPTED, None),
            Closed(
                0,
                INTERRUPTED,
                "interrupted",
                "authentication_failed",
                stamp,
            ),
        ]
    };
    let relaunched = |launch, session| {
        vec![
            Opened(launch, FORKED, Some(session)),
            Closed(launch, FORKED, "completed", "", None),
        ]
    };
    let fresh = |launch| {
        vec![
            Opened(launch, MINTED, None),
            Closed(launch, MINTED, "completed", "", None),
        ]
    };
    let with_task = |note: &str| format!("{note}\n\n{RESUME_TASK}");
    let fresh_after = |ended, class| Relaunch {
        log: login_released(),
        seeded: vec![
            Opened(0, INTERRUPTED, None),
            Closed(0, INTERRUPTED, ended, class, Some(0)),
        ],
        appended: fresh(1),
        tasks: vec![FULL_PROMPT.to_string()],
        answered: (MINTED, ""),
        ..Default::default()
    };
    let undecodable = |type_: &str| Event::new(type_, UNDECODABLE.to_vec());
    vec![
        Relaunch {
            name: "a fresh launch a released hold interrupted",
            log: login_released(),
            seeded: login_close(Some(0)),
            appended: relaunched(1, INTERRUPTED),
            tasks: vec![LOGIN_RELEASED.to_string()],
            answered: (FORKED, INTERRUPTED),
            ..Default::default()
        },
        Relaunch {
            name: "a resume a released hold interrupted, sending its resume task again",
            log: login_released(),
            seeded: vec![
                Opened(0, PRIOR_SESSION, Some(PRIOR_SESSION)),
                Closed(
                    0,
                    PRIOR_SESSION,
                    "interrupted",
                    "authentication_failed",
                    Some(0),
                ),
            ],
            resumed: true,
            appended: relaunched(1, PRIOR_SESSION),
            tasks: vec![with_task(LOGIN_RELEASED)],
            answered: (FORKED, PRIOR_SESSION),
            ..Default::default()
        },
        Relaunch {
            name: "a fallback's fresh launch a released hold interrupted, sent no resume task",
            log: login_released(),
            seeded: vec![
                Opened(0, PRIOR_SESSION, Some(PRIOR_SESSION)),
                Closed(0, PRIOR_SESSION, "interrupted", "", None),
                Opened(1, FELL_BACK, None),
                Closed(
                    1,
                    FELL_BACK,
                    "interrupted",
                    "authentication_failed",
                    Some(0),
                ),
            ],
            resumed: true,
            appended: relaunched(2, FELL_BACK),
            tasks: vec![LOGIN_RELEASED.to_string()],
            answered: (FORKED, FELL_BACK),
            ..Default::default()
        },
        Relaunch {
            name: "a resume the restart's reconciliation closed",
            seeded: vec![
                Opened(0, PRIOR_SESSION, Some(PRIOR_SESSION)),
                Closed(0, PRIOR_SESSION, "interrupted", "", None),
            ],
            resumed: true,
            appended: relaunched(1, PRIOR_SESSION),
            tasks: vec![with_task(RESTARTED)],
            answered: (FORKED, PRIOR_SESSION),
            ..Default::default()
        },
        Relaunch {
            name: "a relaunch of a resume the restart interrupted in its turn, sending its resume \
                task again",
            log: login_released(),
            seeded: vec![
                Opened(0, PRIOR_SESSION, Some(PRIOR_SESSION)),
                Closed(
                    0,
                    PRIOR_SESSION,
                    "interrupted",
                    "authentication_failed",
                    Some(0),
                ),
                Opened(1, RELAUNCHED, Some(PRIOR_SESSION)),
                Closed(1, RELAUNCHED, "interrupted", "", None),
            ],
            resumed: true,
            appended: relaunched(2, RELAUNCHED),
            tasks: vec![with_task(RESTARTED)],
            answered: (FORKED, RELAUNCHED),
            ..Default::default()
        },
        Relaunch {
            name:
                "a relaunch of a fallback the restart interrupted in its turn, sent no resume task",
            log: login_released(),
            seeded: vec![
                Opened(0, PRIOR_SESSION, Some(PRIOR_SESSION)),
                Closed(0, PRIOR_SESSION, "interrupted", "", None),
                Opened(1, FELL_BACK, None),
                Closed(
                    1,
                    FELL_BACK,
                    "interrupted",
                    "authentication_failed",
                    Some(0),
                ),
                Opened(2, RELAUNCHED, Some(FELL_BACK)),
                Closed(2, RELAUNCHED, "interrupted", "", None),
            ],
            resumed: true,
            appended: relaunched(3, RELAUNCHED),
            tasks: vec![RESTARTED.to_string()],
            answered: (FORKED, RELAUNCHED),
            ..Default::default()
        },
        Relaunch {
            name: "an upgraded hold names its latest cause",
            log: vec![
                run_held_event(&hold("rate_limit", "", RETRIES_BY_ITSELF, false, "")),
                run_held_event(&login_hold()),
                released(7),
            ],
            seeded: vec![
                Opened(0, INTERRUPTED, None),
                Closed(0, INTERRUPTED, "interrupted", "rate_limit", Some(0)),
            ],
            appended: relaunched(1, INTERRUPTED),
            tasks: vec!["This session was interrupted: the run was held 7s \
                (authentication_failed - run claude auth login) and is released now. Continue \
                where you left off."
                .to_string()],
            answered: (FORKED, INTERRUPTED),
            ..Default::default()
        },
        Relaunch {
            name: "the first release past the close names the hold",
            log: vec![
                run_held_event(&login_hold()),
                released(42),
                run_held_event(&unstable_hold()),
                released(9),
            ],
            seeded: login_close(Some(0)),
            appended: relaunched(1, INTERRUPTED),
            tasks: vec![LOGIN_RELEASED.to_string()],
            answered: (FORKED, INTERRUPTED),
            ..Default::default()
        },
        Relaunch {
            name: "a close stamped at the hold it repeated",
            log: login_released(),
            seeded: login_close(Some(1)),
            appended: relaunched(1, INTERRUPTED),
            tasks: vec![LOGIN_RELEASED.to_string()],
            answered: (FORKED, INTERRUPTED),
            ..Default::default()
        },
        Relaunch {
            name: "a close stamped at the release read it, so no release follows it",
            log: login_released(),
            seeded: login_close(Some(2)),
            appended: relaunched(1, INTERRUPTED),
            tasks: vec![LOGIN_CLASS.to_string()],
            answered: (FORKED, INTERRUPTED),
            ..Default::default()
        },
        Relaunch {
            name: "an unstamped close",
            log: login_released(),
            seeded: login_close(None),
            appended: relaunched(1, INTERRUPTED),
            tasks: vec![LOGIN_CLASS.to_string()],
            answered: (FORKED, INTERRUPTED),
            ..Default::default()
        },
        Relaunch {
            name: "a host with no hold sink",
            log: login_released(),
            seeded: login_close(Some(0)),
            sink: Sink::Absent,
            appended: relaunched(1, INTERRUPTED),
            tasks: vec![LOGIN_CLASS.to_string()],
            answered: (FORKED, INTERRUPTED),
            ..Default::default()
        },
        Relaunch {
            name: "a release past the close that ends no hold",
            log: vec![released(5)],
            seeded: login_close(Some(0)),
            appended: relaunched(1, INTERRUPTED),
            tasks: vec![LOGIN_CLASS.to_string()],
            answered: (FORKED, INTERRUPTED),
            ..Default::default()
        },
        Relaunch {
            name: "a fault launches fresh",
            ..fresh_after("fault", "unknown")
        },
        Relaunch {
            name: "a stop launches fresh",
            ..fresh_after("stopped", "")
        },
        Relaunch {
            name: "an answer launches fresh",
            ..fresh_after("completed", "")
        },
        Relaunch {
            name: "an open launch launches fresh",
            log: login_released(),
            seeded: vec![Opened(0, INTERRUPTED, None)],
            appended: fresh(1),
            tasks: vec![FULL_PROMPT.to_string()],
            answered: (MINTED, ""),
            ..Default::default()
        },
        Relaunch {
            name: "a prior run's interrupted launch launches fresh",
            log: login_released(),
            seeded: login_close(Some(0)),
            prior_run: true,
            appended: fresh(0),
            tasks: vec![FULL_PROMPT.to_string()],
            answered: (MINTED, ""),
            ..Default::default()
        },
        Relaunch {
            name: "a relaunch with no system/init takes the fallback",
            log: login_released(),
            seeded: login_close(Some(0)),
            resume_ending: "refused-result",
            appended: vec![
                Opened(1, FORKED, Some(INTERRUPTED)),
                Closed(1, FORKED, "interrupted", "", None),
                Opened(2, MINTED, None),
                Closed(2, MINTED, "completed", "", None),
            ],
            tasks: vec![LOGIN_RELEASED.to_string(), FULL_PROMPT.to_string()],
            answered: (MINTED, ""),
            ..Default::default()
        },
        Relaunch {
            name: "a restart's relaunch of a session that never started takes the fallback",
            seeded: vec![
                Opened(0, INTERRUPTED, None),
                Closed(0, INTERRUPTED, "interrupted", "", None),
            ],
            resume_ending: "refused-result",
            appended: vec![
                Opened(1, FORKED, Some(INTERRUPTED)),
                Closed(1, FORKED, "interrupted", "", None),
                Opened(2, MINTED, None),
                Closed(2, MINTED, "completed", "", None),
            ],
            tasks: vec![RESTARTED.to_string(), FULL_PROMPT.to_string()],
            answered: (MINTED, ""),
            ..Default::default()
        },
        // A fresh fallback launch that fails before its open record (write_roots, the persona
        // file, build_args or next_launch) leaves the same latest close as a death does.
        Relaunch {
            name: "a fallback's close a death left the latest is relaunched and falls back again",
            seeded: vec![
                Opened(0, PRIOR_SESSION, Some(PRIOR_SESSION)),
                Closed(0, PRIOR_SESSION, "interrupted", "", None),
            ],
            resumed: true,
            resume_ending: "refused-result",
            appended: vec![
                Opened(1, FORKED, Some(PRIOR_SESSION)),
                Closed(1, FORKED, "interrupted", "", None),
                Opened(2, MINTED, None),
                Closed(2, MINTED, "completed", "", None),
            ],
            tasks: vec![with_task(RESTARTED), FULL_PROMPT.to_string()],
            answered: (MINTED, ""),
            ..Default::default()
        },
        Relaunch {
            name: "a hold sink that cannot be read launches nothing",
            log: login_released(),
            seeded: login_close(Some(0)),
            sink: Sink::Failing,
            failure: "event store: the run read failed".to_string(),
            ..Default::default()
        },
        Relaunch {
            name: "an undecodable release past the close launches nothing",
            log: vec![
                run_held_event(&login_hold()),
                undecodable(rigger::run::TYPE_RUN_RELEASED),
            ],
            seeded: login_close(Some(0)),
            failure: serde_json::from_slice::<rigger::run::RunReleased>(UNDECODABLE)
                .unwrap_err()
                .to_string(),
            ..Default::default()
        },
        Relaunch {
            name: "an undecodable hold before the release launches nothing",
            log: vec![undecodable(TYPE_RUN_HELD), released(42)],
            seeded: login_close(Some(0)),
            failure: serde_json::from_slice::<RunHeld>(UNDECODABLE)
                .unwrap_err()
                .to_string(),
            ..Default::default()
        },
    ]
}

/// THE RELAUNCH: a spawn whose latest launch ended `interrupted`, whichever site closed it,
/// `--resume`s that launch's session as a fork under a session the host mints at the next ordinal
/// in the same directory, its first message the note the close decides (the hold the first
/// release past the close's stamp ended, the supervisor's restart for a close of no class, the
/// class alone otherwise), then the `resume_task` when that launch carried it, as its record says
/// (its session or the session it resumed is the one the spawn names), recording only its answer,
/// so no attempt is charged. Any other latest launch, or a prior run's, launches fresh, a relaunch
/// with no `system/init` takes the fallback, and a hold sink the host cannot read or fold launches
/// nothing.
#[cfg(unix)]
#[test]
fn an_interrupted_launch_relaunches_its_session_as_its_close_decides() {
    for case in relaunch_cases() {
        let name = case.name;
        let fx = Fixture::started(&case.log);
        let run = if case.prior_run { "run-0" } else { "run-1" };
        for &record in &case.seeded {
            let seeded = fx.record(record, "", "");
            rigger::progress_store::record_launch(&fx.progress_store, run, &seeded).unwrap();
        }
        let before = fx.launch_records().len();
        let armed = std::sync::atomic::AtomicBool::new(true);
        let failing = common::fixtures::ReadsFailWhenArmed {
            inner: &fx.run_store,
            armed: &armed,
        };
        let driver = Driver {
            hold_sink: match case.sink {
                Sink::Run => Some(&fx.run_store as &dyn EventStore),
                Sink::Failing => Some(&failing),
                Sink::Absent => None,
            },
            ..fx.stopping_driver(RESUME_ENDING_AGENT, WIND_DOWN_GRACE)
        };
        let base = if case.resumed {
            fx.resumed_opts("", "")
        } else {
            fx.fresh_opts("")
        };
        let o = SpawnOpts {
            env: fx.ending_env(case.resume_ending, "answer"),
            ..base
        };

        let outcome = driver.spawn(&AgentDef::default(), FULL_PROMPT, &o, &no_emit);

        let records = fx.unstamped_records();
        // The sessions the host minted: a relaunch's fork on the first record the spawn appends,
        // a fresh launch on the last.
        let session_of = |record: Option<&SpawnLaunched>| {
            record
                .map(|record| record.session_id.clone())
                .unwrap_or_default()
        };
        let (forked, minted) = (session_of(records.get(before)), session_of(records.last()));
        let appended: Vec<SpawnLaunched> = records[before..]
            .iter()
            .map(|record| SpawnLaunched {
                revision: None,
                ..record.clone()
            })
            .collect();
        let expected: Vec<SpawnLaunched> = case
            .appended
            .iter()
            .map(|&record| fx.record(record, &forked, &minted))
            .collect();
        assert_eq!(appended, expected, "{name}: the records");
        let stands_for = |placeholder: &str| {
            case.appended.iter().any(|record| match *record {
                Record::Opened(_, session, _) | Record::Closed(_, session, ..) => {
                    session == placeholder
                }
            })
        };
        for (placeholder, session) in [(FORKED, &forked), (MINTED, &minted)] {
            if stands_for(placeholder) {
                assert!(
                    ![INTERRUPTED, FELL_BACK, PRIOR_SESSION, RELAUNCHED]
                        .contains(&session.as_str()),
                    "{name}: the session {placeholder} stands for is minted, not {session}"
                );
            }
        }
        if stands_for(FORKED) && stands_for(MINTED) {
            assert_ne!(
                forked, minted,
                "{name}: the fork and the fresh launch mint their own"
            );
        }
        let tasks: Vec<String> = case.tasks.iter().map(|task| framed(task)).collect();
        assert_eq!(fx.tasks_read(), tasks, "{name}: the first messages");
        let session = |id: &str| match id {
            FORKED => forked.clone(),
            MINTED => minted.clone(),
            named => named.to_string(),
        };
        assert_eq!(
            outcome
                .map(|result| (result.session_id, result.resumed_from))
                .map_err(|e| rigger::conductor::strip_failure_marker(&e)),
            if case.failure.is_empty() {
                Ok((session(case.answered.0), session(case.answered.1)))
            } else {
                Err(case.failure.clone())
            },
            "{name}: the outcome"
        );
        let results = if case.failure.is_empty() {
            fresh_answer_on_record()
        } else {
            Vec::new()
        };
        assert_eq!(fx.recorded_results(), results, "{name}: the results");
    }
}

/// THE HOST'S DECISION TRAVELS WITH ITS ERROR (spec 105): a release landing behind the host's held
/// read at a launch's ending, before its close and before the held-run decorator decides on the
/// error, relaunches the interrupted session at the next ordinal: the decorator hands the conductor
/// no error and records no result for it, and the close, stamped from the held read, names the
/// hold that release ended in the relaunch's note. The decorator and the host read the run
/// through one counting store: the decorator's read before the launch is calls 0 to 2, the hold
/// landing behind it, and the host's held read at the ending is calls 3 to 5, the release landing
/// behind it.
#[cfg(unix)]
#[test]
fn a_release_behind_the_hosts_held_read_relaunches_the_session_naming_the_hold() {
    use common::fixtures::{released_event, run_held_event, ReadCountingStore};
    let fx = Fixture::started(&[]);
    let stream = rigger::conductor::STREAM;
    let run = ReadCountingStore::new(&fx.run_store)
        .interleaving(2, stream, vec![run_held_event(&login_hold())])
        .interleaving(
            5,
            stream,
            vec![released_event(rigger::run::HOLD_BY_OPERATOR, 42)],
        );
    let host = Driver {
        hold_sink: Some(&run),
        ..fx.stopping_driver(RESUME_ENDING_AGENT, WIND_DOWN_GRACE)
    };
    let held = rigger::held::HeldDriver::new(&host, &run, None, &|| {});

    let outcome = held.spawn(
        &AgentDef::default(),
        FULL_PROMPT,
        &fx.fresh_opts("init-eof"),
        &no_emit,
    );

    let closes: Vec<(u32, String, Option<String>)> = fx
        .launch_records()
        .into_iter()
        .filter_map(|record| Some((record.launch, record.ended?, record.class)))
        .collect();
    assert_eq!(
        closes,
        vec![
            (0, "interrupted".to_string(), Some("unknown".to_string())),
            (1, "completed".to_string(), None),
        ],
        "the closes"
    );
    assert_eq!(
        fx.tasks_read(),
        vec![framed(FULL_PROMPT), framed(LOGIN_RELEASED)],
        "the first messages"
    );
    assert_eq!(
        outcome
            .map(|result| result.output)
            .map_err(|e| rigger::conductor::strip_failure_marker(&e)),
        Ok(FIXTURE_ANSWER.to_string()),
        "the outcome"
    );
    assert_eq!(
        fx.recorded_results(),
        fresh_answer_on_record(),
        "the results"
    );
}
