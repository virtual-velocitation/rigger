//! Periphery (contract / API / integration) tests for spec 101 criterion 2: ONE-SHOT COMMANDS READ
//! FROM THE BOUNDARY. `rigger status`, `rigger watch`, the dash snapshot, the side-car behind
//! `rigger peers`, the MCP tools and the worker couriers (`prompt`, `scratch`, `result`,
//! `reported`, `hook stop-failure`, `resume-unit`) read the run's own events from its boundary,
//! with the perception types excluded and the carried-over knowledge (decisions, lessons, findings)
//! read by type, and the run's progress from its own stream. These run OUTSIDE the crate and
//! guard what the inside-out tests are structurally blind to:
//!
//!  - the in-crate tests count the read over a bare `:memory:` store; nothing drives the product's
//!    own composition - a project namespace over a file-backed `events.db` another project shares -
//!    and pins that the namespace forwards each typed read to the backend as ONE typed read of its
//!    own scoped stream, so the store (not the caller) refuses what the selection refuses;
//!  - the MCP tools used to hold a side-car subscription; nothing outside the crate pins that each
//!    call now reads afresh (a decision recorded between two calls is seen by the second, at the
//!    cost of exactly one more read), that the spawn-bound `rigger_progress` stamps the run the
//!    boundary read names, nor that `rigger_activity` reads that run's progress stream alone;
//!  - no counting double reaches into the compiled binary, so the binary tests make any read past
//!    the slice observable instead: every derived event and every superseded run's own event in a
//!    real 200,000-event log is made undecodable, so a command that materialized even one of them
//!    would fail - and a control proves the poison does fail a command that reads it.

mod common;

use std::io::Cursor;
use std::path::Path;
use std::process::Stdio;

use common::cli::{rigger_file, run_rigger, run_stream_identity, temp_store_project};
use common::fixtures::{
    ev, generation_ingested, run_started, seed_one_shot_fixture, seed_one_shot_progress, types_of,
    CountedRead, HandBuiltLog, OneShotFixture, ReadCountingStore, ONE_SHOT_DERIVED_TYPES,
    ONE_SHOT_PERCEPTION_TYPES,
};
use rigger::conductor::STREAM;
use rigger::driver::workflow::Driver;
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore, ExpectedRevision, TypeSelection};
use rigger::mcpserver::Server;
use serde_json::{json, Value};

/// The carried-over types, spelled out: a run read hands back every one of them from every run.
const CARRY_OVER: [&str; 3] = ["DecisionMade", "LessonLearned", "ReviewFinding"];

/// The `id` field of each event's JSON payload, in order (empty when it has none).
fn payload_ids(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .map(|e| {
            let body: Value = serde_json::from_slice(&e.data).unwrap_or(Value::Null);
            body["id"].as_str().unwrap_or_default().to_string()
        })
        .collect()
}

/// `beta`'s history in a shared `events.db`: a run started as `run`, a decision, a lesson, a
/// derived edge and a note - a neighbor whose records no read of another project's run may hold.
fn seed_beta_history(beta: &dyn EventStore, run: &str, decision: &str) {
    beta.append(
        STREAM,
        ExpectedRevision::Any,
        &[
            Event::new("RunStarted", format!(r#"{{"run":"{run}"}}"#).into_bytes())
                .with_meta("run_id", run),
            ev("DecisionMade", &format!(r#"{{"id":"{decision}"}}"#)),
            ev("LessonLearned", r#"{"id":"l-beta"}"#),
            ev("EdgeInferred", r#"{"from":"a","rel":"CALLS","to":"b"}"#),
            ev("RunNote", "{}"),
        ],
    )
    .unwrap();
}

/// Given one `events.db` file two projects share - `alpha` holding two superseded runs and 200,000
/// derived events before its current run's boundary, `beta` holding runs, decisions and derived
/// events of its own on both sides of alpha's history - when alpha's run is read through the
/// product's composition (a project namespace over the file-backed store), then:
///  - counted ABOVE the namespace, the read is exactly the boundary lookup, the carried-over
///    knowledge by type and the run slice from the boundary, materializing the run's own events
///    plus the carry-over and nothing of beta;
///  - counted BELOW the namespace, the backend is asked the same three questions of alpha's
///    scoped stream with the same selections - the typed read reaches the store as a typed read,
///    so the store refuses the perception types, never the caller after materializing them.
#[test]
fn a_project_namespace_over_a_shared_events_file_reads_its_run_as_one_typed_read_per_selection() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("events.db");
    let backend = Store::open(db.to_str().unwrap()).unwrap();
    let alpha = Namespaced::new(&backend, "alpha");
    let beta = Namespaced::new(&backend, "beta");
    seed_beta_history(&beta, "beta-1", "d-beta-1");
    let fixture = seed_one_shot_fixture(&alpha, STREAM, &["the current campaign"]);
    seed_beta_history(&beta, "beta-2", "d-beta-2");

    let above = ReadCountingStore::new(&alpha);
    let events = rigger::run::read::read_run(&above, STREAM).unwrap();
    assert_eq!(above.reads(), fixture.read(STREAM));
    assert_eq!(above.materialized(), fixture.cost());
    assert_eq!(
        types_of(&events),
        [
            "DecisionMade",
            "LessonLearned",
            "ReviewFinding",
            "RunStarted",
            "RunNote",
            "DecisionMade",
            "ReviewFinding",
            "RunNote",
        ]
    );
    assert_eq!(
        payload_ids(&events),
        ["d-a", "l-a", "f-b", "", "", "d-c", "f-c", ""],
        "every run's carry-over and the current run, never beta's"
    );
    assert!(
        events.iter().all(|e| e.stream == STREAM),
        "the namespace hands the stream back unprefixed"
    );
    assert_eq!(events[3].revision, fixture.boundary);

    let counted_backend = ReadCountingStore::new(&backend);
    let below = Namespaced::new(&counted_backend, "alpha");
    let again = rigger::run::read::read_run(&below, STREAM).unwrap();
    let scoped = format!("{}{STREAM}", Namespaced::prefix_for("alpha"));
    assert_eq!(counted_backend.reads(), fixture.read(&scoped));
    assert_eq!(counted_backend.materialized(), fixture.cost());
    assert_eq!(
        payload_ids(&again),
        payload_ids(&events),
        "the same run whichever side of the namespace counts it"
    );
}

/// One JSON-RPC `tools/call` of `name` with `args` against `server`, answered in full.
fn call_tool(server: &Server, name: &str, args: Value) -> Value {
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {"name": name, "arguments": args},
    });
    let mut out = Vec::new();
    server
        .run(Cursor::new(format!("{request}\n")), &mut out)
        .unwrap();
    serde_json::from_slice(&out).unwrap_or_else(|e| {
        panic!(
            "{name} answers one JSON reply ({e}): {}",
            String::from_utf8_lossy(&out)
        )
    })
}

/// `(id, live)` of each decision a `rigger_peers` reply holds, in order.
fn live_decisions(reply: &Value) -> Vec<(String, bool)> {
    reply["result"]["structuredContent"]["decisions"]
        .as_array()
        .unwrap_or_else(|| panic!("a decisions array: {reply}"))
        .iter()
        .map(|d| {
            (
                d["id"].as_str().unwrap().to_string(),
                d["live"].as_bool().unwrap(),
            )
        })
        .collect()
}

/// The `id`s in one section of a `rigger_peers` reply, in order.
fn section_ids(reply: &Value, section: &str) -> Vec<String> {
    reply["result"]["structuredContent"][section]
        .as_array()
        .unwrap_or_else(|| panic!("a {section} array: {reply}"))
        .iter()
        .map(|v| v["id"].as_str().unwrap().to_string())
        .collect()
}

/// Given a spawn-bound MCP server over the 200,000-derived-event log, when an agent calls
/// `rigger_peers` scoped to a file, a peer then records a decision about that file, and the agent
/// calls `rigger_peers` again and reports progress, then every call is exactly one read of the
/// run from its boundary plus the typed carry-over - the second peers call sees the new decision
/// LIVE at the cost of one more carried-over event, `rigger_progress` stamps its report with
/// the run the boundary read names, and `rigger_activity` reads the run's progress from its own
/// stream alone.
#[test]
fn every_mcp_peers_and_progress_call_reads_the_run_afresh_from_its_boundary() {
    let inner = Store::open(":memory:").unwrap();
    let fixture = seed_one_shot_fixture(&inner, STREAM, &["the current campaign"]);
    let store = ReadCountingStore::new(&inner);
    let progress_inner = Store::open(":memory:").unwrap();
    seed_one_shot_progress(&progress_inner, 1);
    let progress = ReadCountingStore::new(&progress_inner);
    let scratch = tempfile::tempdir().unwrap();
    let driver = Driver::new();
    let server = Server::new(&driver, &store, STREAM)
        .with_progress(&progress, scratch.path().to_str().unwrap())
        .with_spawn("u/implementer#0");

    let first = call_tool(&server, "rigger_peers", json!({"files": ["c.rs"]}));
    assert_eq!(live_decisions(&first), [("d-c".to_string(), true)]);
    assert_eq!(section_ids(&first, "findings"), ["f-c"]);
    assert_eq!(section_ids(&first, "lessons"), Vec::<String>::new());
    assert_eq!(store.reads(), fixture.read(STREAM));

    inner
        .append(
            STREAM,
            ExpectedRevision::Any,
            &[ev(
                "DecisionMade",
                r#"{"id":"d-new","summary":"chose new","governs":["c.rs"]}"#,
            )],
        )
        .unwrap();
    let after = OneShotFixture {
        run_events: fixture.run_events + 1,
        carry_over: fixture.carry_over + 1,
        ..fixture
    };

    let second = call_tool(&server, "rigger_peers", json!({"files": ["c.rs"]}));
    assert_eq!(
        live_decisions(&second),
        [("d-c".to_string(), true), ("d-new".to_string(), true)],
        "a decision recorded between two calls is seen by the second"
    );
    assert_eq!(
        store.reads(),
        [fixture.read(STREAM), after.read(STREAM)].concat()
    );

    let reported = call_tool(&server, "rigger_progress", json!({"activity": "probing"}));
    assert_eq!(reported["result"]["structuredContent"], json!({}));
    assert_eq!(
        store.reads(),
        [fixture.read(STREAM), after.read(STREAM), after.read(STREAM)].concat()
    );
    assert_eq!(store.materialized(), fixture.cost() + 2 * after.cost());
    assert_eq!(progress.reads(), [], "recording progress reads nothing");

    // `rigger_activity` (the workflow surface): one read of the run, and ONE read of the run's
    // own progress stream -
    // its seeded report and the one just recorded - never a superseded run's reports.
    let workflow = Server::new(&driver, &store, STREAM)
        .with_progress(&progress, scratch.path().to_str().unwrap());
    let activity = call_tool(&workflow, "rigger_activity", json!({}));
    assert!(activity.get("result").is_some(), "{activity}");
    assert_eq!(
        store.reads(),
        [
            fixture.read(STREAM),
            after.read(STREAM),
            after.read(STREAM),
            after.read(STREAM)
        ]
        .concat()
    );
    assert_eq!(
        progress.reads(),
        [CountedRead::Stream {
            stream: "progress/run-c".to_string(),
            from: 0,
            forward: true,
            materialized: 2,
        }]
    );
    let recorded = rigger::progress::read_run(&progress_inner, "run-c").unwrap();
    assert_eq!(
        recorded
            .iter()
            .map(|e| e.meta.get("run_id").map(String::as_str))
            .collect::<Vec<_>>(),
        [Some("run-c"), Some("run-c")],
        "the report is stamped with the run the boundary read names, on that run's stream"
    );
}

/// `root`'s run stream as the compiled binary names it inside `events.db`.
fn scoped_run_stream(root: &Path) -> String {
    format!(
        "{}{STREAM}",
        Namespaced::prefix_for(&run_stream_identity(root))
    )
}

/// Rewrite every event on `root`'s run stream that `condition` (an SQL predicate over the
/// `events` row) selects with `set` (an SQL assignment), around the store's own write guards - a
/// log state only a stale or broken writer leaves. Returns how many rows it rewrote.
fn rewrite_run_stream(root: &Path, set: &str, condition: &str) -> usize {
    let conn = rusqlite::Connection::open(rigger_file(root, "events.db")).unwrap();
    conn.execute(
        &format!("UPDATE events SET {set} WHERE stream = ?1 AND ({condition})"),
        [scoped_run_stream(root)],
    )
    .unwrap()
}

/// Make every event on `root`'s run stream that `condition` (an SQL predicate over the `events`
/// row) selects undecodable: its `meta` becomes a blob no read can turn back into an event, so any
/// command that materializes one fails. Returns how many it poisoned.
fn poison(root: &Path, condition: &str) -> usize {
    rewrite_run_stream(root, "meta = X'ff'", condition)
}

/// `rigger <args>` in `root`, which must succeed; its stdout.
fn rigger_ok(root: &Path, args: &[&str]) -> String {
    let (out, err, ok) = run_rigger(root, args);
    assert!(ok, "rigger {args:?} must succeed; stderr:\n{err}");
    out
}

/// `rigger mcp` in `root`, answering one `rigger_peers` call on stdin; the reply's structured
/// content. The child is reaped through its own handle.
fn mcp_peers(root: &Path) -> Value {
    use std::io::Write;
    let mut child = common::rigger_courier()
        .arg("mcp")
        .current_dir(root)
        .env("RIGGER_NO_DASH", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn `rigger mcp`");
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {"name": "rigger_peers", "arguments": {}},
    });
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("{request}\n").as_bytes())
        .unwrap();
    let out = child.wait_with_output().expect("`rigger mcp` exits at EOF");
    assert!(
        out.status.success(),
        "rigger mcp: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let reply: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "one JSON reply ({e}): {}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
    reply["result"]["structuredContent"].clone()
}

/// Seed `root`'s `events.db` with the one-shot fixture under the project's namespace, then
/// `current` (the current run's further events, appended through the same namespace), and make
/// every derived event and every superseded run's event outside the `read` types undecodable -
/// so a command that materializes even one of them fails. Returns how many superseded events it
/// poisoned.
fn seed_poisoned_project(
    root: &Path,
    criteria: &[&str],
    read: &[&str],
    current: impl FnOnce(&dyn EventStore),
) -> usize {
    let fixture = {
        let backend = Store::open(rigger_file(root, "events.db").to_str().unwrap()).unwrap();
        let store = Namespaced::new(&backend, &run_stream_identity(root));
        let fixture = seed_one_shot_fixture(&store, STREAM, criteria);
        current(&store);
        fixture
    };
    let quoted = |types: &[&str]| {
        types
            .iter()
            .map(|t| format!("'{t}'"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let derived = poison(
        root,
        &format!("type IN ({})", quoted(&ONE_SHOT_DERIVED_TYPES)),
    );
    assert_eq!(derived, 200_100, "every derived event is poisoned");
    poison(
        root,
        &format!(
            "revision < {} AND type NOT IN ({}, {})",
            fixture.boundary,
            quoted(read),
            quoted(&ONE_SHOT_DERIVED_TYPES)
        ),
    )
}

/// Given a project whose `events.db` holds two superseded runs and 200,000 derived events before
/// the current run's boundary, with every derived event and every superseded run's own event made
/// undecodable, when the operator runs the one-shot commands, then each answers from the current
/// run and every run's carried-over knowledge - a command that read past its slice would have
/// materialized a poisoned event and failed. A control then poisons a carried-over event and the
/// run's own events, and the commands that read them fail: the poison is live, not inert.
#[test]
fn the_one_shot_commands_answer_from_the_run_without_materializing_a_derived_or_superseded_event() {
    let dir = temp_store_project();
    let root = dir.path();
    assert_eq!(
        seed_poisoned_project(root, &["the current campaign"], &CARRY_OVER, |_| {}),
        6,
        "both superseded runs' RunStarted, UnitStarted and note are poisoned"
    );

    // `rigger peers`: every run's decisions, lessons and findings, the current run's LIVE.
    assert_eq!(
        rigger_ok(root, &["peers"]),
        "decision d-a | HISTORICAL | chose d-a | governs: a.rs\n\
         decision d-c | LIVE | chose d-c | governs: c.rs\n\
         lesson l-a | learned a | about: a.rs\n\
         finding f-b | by lens | found b | about: b.rs\n\
         finding f-c | by lens | found c | about: c.rs\n"
    );
    assert_eq!(
        rigger_ok(root, &["peers", "c.rs"]),
        "decision d-c | LIVE | chose d-c | governs: c.rs\n\
         finding f-c | by lens | found c | about: c.rs\n"
    );

    // `rigger mcp`'s `rigger_peers`: the same records, answered on the first call.
    assert_eq!(
        mcp_peers(root),
        json!({
            "decisions": [
                {"id": "d-a", "summary": "chose d-a", "governs": ["a.rs"], "live": false},
                {"id": "d-c", "summary": "chose d-c", "governs": ["c.rs"], "live": true},
            ],
            "lessons": [{"id": "l-a", "summary": "learned a", "about": ["a.rs"]}],
            "findings": [
                {"id": "f-b", "by": "lens", "summary": "found b", "about": ["b.rs"]},
                {"id": "f-c", "by": "lens", "summary": "found c", "about": ["c.rs"]},
            ],
        })
    );

    // `rigger prime`: every run's decisions by type, newest first, after its instructions line.
    assert_eq!(
        rigger_ok(root, &["prime"])
            .lines()
            .skip(1)
            .collect::<Vec<_>>(),
        [
            "# Rigger: recent decisions",
            "- d-c: chose d-c",
            "- d-a: chose d-a"
        ]
    );

    // `rigger status`: the current run, by its id.
    let status = rigger_ok(root, &["status"]);
    assert_eq!(
        status.lines().take(2).collect::<Vec<_>>(),
        ["run run-c", "- . 0/0 units . healthy"]
    );
    assert_eq!(rigger_ok(root, &["status", "--json"]), "[]\n");

    // `rigger watch --once`: a healthy run reports nothing.
    assert_eq!(rigger_ok(root, &["watch", "--once"]), "");

    // The dash snapshot: the current run alone.
    assert_eq!(
        rigger_ok(root, &["dash", "--export", "snapshot.html"]),
        "wrote dash snapshot to snapshot.html\n"
    );
    let html = std::fs::read_to_string(root.join("snapshot.html")).unwrap();
    assert!(
        html.contains("run-c") && html.contains("the current campaign"),
        "the snapshot shows the current run"
    );
    assert!(
        !html.contains("run-a") && !html.contains("run-b") && !html.contains("prior campaign"),
        "the snapshot shows no superseded run"
    );

    // `rigger progress`: the report is stamped with the run the boundary read names.
    rigger_ok(root, &["progress", "u/implementer#0", "probing"]);
    let progress_db = Store::open(rigger_file(root, "progress.db").to_str().unwrap()).unwrap();
    let reports = rigger::progress::read_run(
        &Namespaced::new(&progress_db, &run_stream_identity(root)),
        "run-c",
    )
    .unwrap();
    assert_eq!(
        reports
            .iter()
            .map(|e| e.meta.get("run_id").map(String::as_str))
            .collect::<Vec<_>>(),
        [Some("run-c")]
    );

    // Control: a poisoned carried-over event is read, so `rigger peers` fails on it ...
    assert_eq!(poison(root, "type = 'LessonLearned'"), 1);
    let (_out, _err, ok) = run_rigger(root, &["peers"]);
    assert!(!ok, "a command that materializes a poisoned event fails");
    // ... and poisoned events of the run's own fail `rigger status`.
    assert_eq!(poison(root, "type = 'RunNote' AND meta != X'ff'"), 2);
    let (_out, _err, ok) = run_rigger(root, &["status"]);
    assert!(
        !ok,
        "a command that materializes a poisoned run event fails"
    );
}

/// Given the same poisoned project with a spawn parked and a unit escalated in the current run,
/// when a worker's couriers run - `rigger prompt`, `rigger scratch`, `rigger reported`, `rigger
/// hook stop-failure`, `rigger result` (`--if-absent`, and `--supersede` over the standing
/// result) - and the operator runs `rigger resume-unit`, then each answers from the current run:
/// a courier that read past the run's slice would have materialized a poisoned event and failed.
#[test]
fn the_worker_couriers_answer_from_the_run_without_materializing_a_derived_or_superseded_event() {
    let dir = temp_store_project();
    let root = dir.path();
    let spawn = "u/implementer#0";
    seed_poisoned_project(root, &["the current campaign"], &CARRY_OVER, |store| {
        rigger::spawn_store::park_in_run(
            store,
            &rigger::spawn::SpawnRequest {
                id: spawn.to_string(),
                unit: "u".to_string(),
                stage: "u".to_string(),
                prompt: "probe the run".to_string(),
                ..Default::default()
            },
            "run-c",
            "",
        )
        .unwrap();
        store
            .append(
                STREAM,
                ExpectedRevision::Any,
                &[
                    ev("UnitStarted", r#"{"id":"uc"}"#).with_meta("run_id", "run-c"),
                    ev("UnitEscalated", r#"{"id":"uc"}"#).with_meta("run_id", "run-c"),
                ],
            )
            .unwrap();
    });

    assert_eq!(rigger_ok(root, &["prompt", spawn]), "probe the run\n");
    let scratch = rigger_ok(root, &["scratch", spawn]);
    assert!(
        scratch.trim_end().ends_with("run-c/u_2fimplementer_230"),
        "the spawn's scratch sits under the current run: {scratch}"
    );
    let (_out, err, ok) = run_rigger(root, &["reported", spawn]);
    assert!(!ok, "no result is recorded yet");
    assert!(
        err.contains("has no recorded result yet"),
        "reported answers from the run, not from a poisoned read: {err}"
    );
    let hooked = rigger_ok(
        root,
        &[
            "hook",
            "stop-failure",
            "--spawn",
            spawn,
            "--class",
            "rate_limit",
        ],
    );
    assert!(
        hooked.starts_with("stop-failure recorded for u/implementer#0"),
        "{hooked}"
    );
    let recorded = rigger_ok(root, &["result", spawn, "--if-absent", "done"]);
    assert!(
        recorded.starts_with("recorded result for u/implementer#0"),
        "{recorded}"
    );
    assert_eq!(
        rigger_ok(root, &["result", spawn, "--if-absent", "again"]),
        "u/implementer#0 already has a result; --if-absent left it untouched\n"
    );
    assert_eq!(
        rigger_ok(root, &["reported", spawn]),
        "u/implementer#0 ok\n"
    );
    let rerecorded = rigger_ok(root, &["result", spawn, "--supersede", "done again"]);
    assert!(
        rerecorded.starts_with("recorded result for u/implementer#0"),
        "{rerecorded}"
    );
    let resumed = rigger_ok(root, &["resume-unit", "uc"]);
    assert!(resumed.starts_with("resumed unit \"uc\""), "{resumed}");
}

/// Given a project that is NOT a git repo (so the step walks no tree and ingests nothing) whose
/// `events.db` holds two superseded runs and 200,000 derived events before the current run's
/// boundary, with every derived event and every superseded run's own event made undecodable, when
/// the operator runs `rigger step`, then the step parks the stage's spawn in the current run, and
/// after the spawn's result is recorded the next `rigger step` replays it and finishes the run -
/// a step that read past the run's slice, or took the ingest seed's derived read without
/// ingesting, would have materialized a poisoned event and failed.
#[test]
fn a_step_that_does_not_ingest_advances_the_run_without_materializing_a_derived_or_superseded_event(
) {
    let dir = common::cli::temp_repoless_project();
    let root = dir.path();
    common::cli::seed_store(root);
    common::cli::write_workflow(root, "");
    // Started over no criteria, as a step given no spec starts one, so the step adopts it.
    seed_poisoned_project(root, &[], &CARRY_OVER, |_| {});

    assert_eq!(
        rigger_ok(root, &["step"]),
        concat!(
            r#"{"wave":[{"id":"a/implementer#0","unit":"a","stage":"a","model":"sonnet","#,
            r#""tools":["Read","Edit"],"dir":"","max_wall_clock":null,"marker_path":null,"#,
            r#""cargo_target_dir":null}],"done":false}"#,
            "\n"
        ),
        "the first step parks the stage's spawn"
    );
    rigger_ok(root, &["result", "a/implementer#0", "done"]);
    assert_eq!(
        rigger_ok(root, &["step"]),
        "{\"wave\":[],\"done\":true}\n",
        "the second step replays the result and finishes the run"
    );
    assert_eq!(
        rigger_ok(root, &["status"])
            .lines()
            .take(2)
            .collect::<Vec<_>>(),
        ["run run-c", "a . 0/1 units . working"],
        "the step advanced the run it adopted - no new run was minted (the stage's `on_pass: none` \
         lands nothing, so its unit is not counted integrated)"
    );

    // Control: the step does materialize the run's own events, so poisoning one fails it - the
    // poison is live for the step, not inert.
    assert_eq!(poison(root, "type = 'RunNote' AND meta != X'ff'"), 2);
    let (_out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        !ok,
        "a step that materializes a poisoned run event fails: {err}"
    );
}

/// Given a git repo whose `events.db` holds two superseded runs and 200,000 derived events before
/// the current run's boundary, with every derived event and every superseded run's event outside
/// the carried-over and criterion-adoption types made undecodable, when the operator runs `rigger
/// step --spec` over the current run's criterion, then the step starts the criterion's unit and
/// parks its spawn in the current run - it read the superseded runs' lifecycle events for
/// adoption, by type, and nothing else of theirs. A control then poisons the superseded runs'
/// `RunStarted` and a fresh step's adoption read fails on it: the adoption read is live.
///
/// The light lane only: in the default lane a repo step folds the tree into the graph and its
/// latest-generation seed is criterion 3's, not this criterion's.
#[cfg(not(feature = "symbols"))]
#[test]
fn a_repo_step_that_starts_a_criterion_unit_reads_only_the_run_the_carry_over_and_adoption_by_type()
{
    /// What a repo step that starts a criterion unit reads of a superseded run, spelled out: the
    /// carried-over types and the criterion-adoption lifecycle types.
    const CARRY_OVER_AND_ADOPTION: [&str; 8] = [
        "DecisionMade",
        "LessonLearned",
        "ReviewFinding",
        "RunStarted",
        "UnitStarted",
        "UnitIntegrated",
        "UnitFailed",
        "UnitStatus",
    ];
    let criterion = "alpha lands cleanly";
    let scaffold = |root: &Path| {
        common::cli::seed_store(root);
        common::cli::write_scaffold(
            root,
            &[("worker", common::cli::UNISOLATED_WORKER)],
            "defaults:\n  grounder: nop\n  budget: 60\n\
             stages:\n  implement:\n    agent: worker\n    strategy: fan-out\n    on_pass: none\n",
        );
        std::fs::write(
            root.join("spec.md"),
            format!("# Spec\n\n## Done when\n\n- [ ] {criterion}\n"),
        )
        .unwrap();
    };
    let dir = common::git::temp_git_project_with_commit();
    let root = dir.path();
    scaffold(root);
    assert_eq!(
        seed_poisoned_project(root, &[criterion], &CARRY_OVER_AND_ADOPTION, |_| {}),
        2,
        "both superseded runs' notes are poisoned"
    );

    let step = rigger_ok(root, &["step", "--spec", "spec.md"]);
    assert!(
        step.contains(r#""id":"unit-1-alpha-lands-cleanly/implementer#0""#)
            && step.contains(r#""done":false"#),
        "the step parks the criterion unit's spawn: {step}"
    );
    assert_eq!(
        rigger_ok(root, &["status"]).lines().next(),
        Some("run run-c"),
        "the step advanced the run it adopted"
    );

    // Control: a fresh step over the same log with the superseded runs' `RunStarted` poisoned too.
    let control = common::git::temp_git_project_with_commit();
    scaffold(control.path());
    seed_poisoned_project(
        control.path(),
        &[criterion],
        &CARRY_OVER_AND_ADOPTION,
        |_| {},
    );
    assert_eq!(
        poison(
            control.path(),
            r#"type = 'RunStarted' AND (meta LIKE '%"run-a"%' OR meta LIKE '%"run-b"%')"#
        ),
        2
    );
    let (_out, err, ok) = run_rigger(control.path(), &["step", "--spec", "spec.md"]);
    assert!(
        !ok,
        "a step whose adoption read meets a poisoned event fails: {err}"
    );
}

/// Given a live run whose earlier progress reports a binary predating the per-run progress
/// streams wrote to the one shared `progress` stream, when the binary is refreshed and the run
/// reads its progress, then those earlier reports are not read (the run's own stream holds none
/// of them) while the next report is - and the handbook tells the operator so.
#[test]
fn a_refresh_to_per_run_progress_streams_drops_a_live_runs_earlier_reports_and_the_handbook_says_so(
) {
    let store = Store::open(":memory:").unwrap();
    let body = json!({"id": "u/implementer#0", "activity": "before the refresh"});
    store
        .append(
            "progress",
            ExpectedRevision::Any,
            &[
                Event::new("AgentProgress", serde_json::to_vec(&body).unwrap())
                    .with_meta("run_id", "run-c"),
            ],
        )
        .unwrap();
    assert_eq!(
        rigger::progress::read_run(&store, "run-c").unwrap().len(),
        0
    );

    rigger::progress_store::record(&store, "run-c", "u/implementer#0", "after the refresh")
        .unwrap();
    let reports = rigger::progress::read_run(&store, "run-c").unwrap();
    assert_eq!(
        reports
            .iter()
            .map(|e| serde_json::from_slice::<Value>(&e.data).unwrap()["activity"].clone())
            .collect::<Vec<_>>(),
        [json!("after the refresh")]
    );

    let handbook = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/handbook/tools-and-context.md"),
    )
    .unwrap();
    assert!(
        handbook.contains(
            "Refreshing the binary to one with per-run progress streams in the middle of a run \
             drops that run's earlier reports from these views"
        ),
        "the handbook names the refresh drop"
    );
}

/// Given one `events.db` two projects share, where `beta` has started a run and `alpha` has not,
/// when `alpha`'s current run is read through the product's composition (a project namespace over
/// the file-backed store), then alpha names NO run (beta's `RunStarted` is never alpha's boundary)
/// and its whole stream but the perception types is its run, in one boundary lookup and one typed
/// read. Once alpha holds the one-shot fixture, the same read names alpha's run and hands back its
/// slice alone, costing exactly the run's own events plus the typed carry-over.
#[test]
fn a_current_run_read_through_a_shared_events_file_names_only_its_own_projects_run() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("events.db");
    let backend = Store::open(db.to_str().unwrap()).unwrap();
    let alpha = Namespaced::new(&backend, "alpha");
    let beta = Namespaced::new(&backend, "beta");
    seed_beta_history(&beta, "beta-1", "d-beta-1");
    alpha
        .append(
            STREAM,
            ExpectedRevision::Any,
            &[
                ev("UnitStarted", r#"{"id":"u0"}"#),
                ev("EdgeInferred", r#"{"from":"a","rel":"CALLS","to":"b"}"#),
                ev("DecisionMade", r#"{"id":"d-0"}"#),
            ],
        )
        .unwrap();
    seed_beta_history(&beta, "beta-2", "d-beta-2");

    let counted = ReadCountingStore::new(&alpha);
    let (events, run_id) = rigger::run::read::read_current_run(&counted, STREAM).unwrap();
    assert_eq!(
        run_id, "",
        "no run started names no run, never a neighbor's"
    );
    assert_eq!(types_of(&events), ["UnitStarted", "DecisionMade"]);
    assert_eq!(payload_ids(&events), ["u0", "d-0"]);
    assert_eq!(
        counted.reads(),
        [
            CountedRead::LastPosition {
                stream: STREAM.to_string(),
                event_type: "RunStarted".to_string(),
            },
            CountedRead::Typed {
                stream: STREAM.to_string(),
                from: 0,
                only: false,
                types: ONE_SHOT_PERCEPTION_TYPES.map(String::from).to_vec(),
                materialized: 2,
            },
        ]
    );

    let gamma = Namespaced::new(&backend, "gamma");
    let fixture = seed_one_shot_fixture(&gamma, STREAM, &["the current campaign"]);
    seed_beta_history(&beta, "beta-3", "d-beta-3");
    let counted = ReadCountingStore::new(&gamma);
    let (events, run_id) = rigger::run::read::read_current_run(&counted, STREAM).unwrap();
    assert_eq!(run_id, "run-c");
    assert_eq!(
        types_of(&events),
        [
            "RunStarted",
            "RunNote",
            "DecisionMade",
            "ReviewFinding",
            "RunNote"
        ]
    );
    assert_eq!(payload_ids(&events), ["", "", "d-c", "f-c", ""]);
    assert_eq!(events[0].revision, fixture.boundary);
    assert_eq!(counted.reads(), fixture.read(STREAM));
    assert_eq!(counted.materialized(), fixture.cost());
}

/// Given the one-shot fixture under `alpha` in a shared `events.db`, when a concurrent writer
/// appends to the backend BELOW the project namespace at each point of one read of alpha's run,
/// then the read is a log prefix of alpha's run whatever lands: alpha's own write (a decision,
/// then a spawn result) before either typed read, or between them, is read whole and once, in log
/// order; one after the last read is not read at all; and a neighbor's write at the same point is
/// never read and costs alpha nothing.
#[test]
fn a_run_read_through_the_namespace_is_a_log_prefix_whatever_a_concurrent_writer_appends() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("events.db");
    let backend = Store::open(db.to_str().unwrap()).unwrap();
    let fixture = seed_one_shot_fixture(
        &Namespaced::new(&backend, "alpha"),
        STREAM,
        &["the current campaign"],
    );
    let alpha_run = format!("{}{STREAM}", Namespaced::prefix_for("alpha"));
    let beta_run = format!("{}{STREAM}", Namespaced::prefix_for("beta"));
    let late = |tag: &str| {
        vec![
            ev("DecisionMade", &format!(r#"{{"id":"d-{tag}"}}"#)),
            ev("SpawnResult", &format!(r#"{{"id":"s-{tag}"}}"#)),
        ]
    };
    let before = ["d-a", "l-a", "f-b", "", "", "d-c", "f-c", ""];
    let owned = |ids: &[&str]| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>();
    let read_with_writer = |after: usize, stream: &str, tag: &str| -> (Vec<String>, usize) {
        let counted = ReadCountingStore::new(&backend).interleaving(after, stream, late(tag));
        let events =
            rigger::run::read::read_run(&Namespaced::new(&counted, "alpha"), STREAM).unwrap();
        let positions: Vec<u64> = events.iter().map(|e| e.position).collect();
        let mut ordered = positions.clone();
        ordered.sort_unstable();
        ordered.dedup();
        assert_eq!(positions, ordered, "each event once, in log order");
        (payload_ids(&events), counted.materialized())
    };

    assert_eq!(
        read_with_writer(0, &alpha_run, "w0"),
        (
            owned(&[&before[..], &["d-w0", "s-w0"]].concat()),
            fixture.cost() + 3
        ),
        "a write before the carry-over read: its decision is read by both typed reads, held once"
    );
    assert_eq!(
        read_with_writer(1, &alpha_run, "w1"),
        (
            owned(&[&before[..], &["d-w0", "s-w0", "d-w1", "s-w1"]].concat()),
            fixture.cost() + 2 + 1 + 2
        ),
        "a write between the typed reads is in the slice: read whole, never a decision missing \
         beside a later result"
    );
    assert_eq!(
        read_with_writer(2, &alpha_run, "w2").0,
        [&before[..], &["d-w0", "s-w0", "d-w1", "s-w1"]].concat(),
        "a write after the last read is not read"
    );
    let (ids, cost) = read_with_writer(1, &beta_run, "beta");
    assert_eq!(
        ids,
        [
            &before[..],
            &["d-w0", "s-w0", "d-w1", "s-w1", "d-w2", "s-w2"]
        ]
        .concat(),
        "a neighbor's write is never read"
    );
    assert_eq!(
        cost,
        fixture.cost() + 3 + 3 + 3,
        "a neighbor's write costs alpha nothing"
    );
}

/// Given one `events.db` holding eleven decisions of this project (and one whose payload is not a
/// decision) and a neighbor project's decisions recorded after them, when a session starts and
/// runs `rigger prime`, then it prints exactly this project's ten newest decisions, newest first -
/// the neighbor's are never read into it, and the unreadable payload is skipped without taking a
/// slot.
#[test]
fn prime_prints_this_projects_ten_newest_decisions_never_a_neighbors() {
    let dir = temp_store_project();
    let root = dir.path();
    {
        let backend = Store::open(rigger_file(root, "events.db").to_str().unwrap()).unwrap();
        let own = Namespaced::new(&backend, &run_stream_identity(root));
        let decisions: Vec<Event> = (1..=11)
            .map(|i| {
                ev(
                    "DecisionMade",
                    &format!(r#"{{"id":"d-{i:02}","summary":"chose {i:02}","governs":["x.rs"]}}"#),
                )
            })
            .chain([ev("DecisionMade", r#"{"note":"not a decision"}"#)])
            .collect();
        own.append(STREAM, ExpectedRevision::Any, &decisions)
            .unwrap();
        let beta = Namespaced::new(&backend, "beta");
        beta.append(
            STREAM,
            ExpectedRevision::Any,
            &[ev(
                "DecisionMade",
                r#"{"id":"d-beta","summary":"a neighbor's","governs":["x.rs"]}"#,
            )],
        )
        .unwrap();
    }

    let expected: Vec<String> = std::iter::once("# Rigger: recent decisions".to_string())
        .chain((2..=11).rev().map(|i| format!("- d-{i:02}: chose {i:02}")))
        .collect();
    assert_eq!(
        rigger_ok(root, &["prime"])
            .lines()
            .skip(1)
            .collect::<Vec<_>>(),
        expected
    );
}

/// Given a project whose run stream holds a disorder a stale writer left in a run BEFORE the
/// current run's boundary, when the operator polls `rigger watch --once`, then the poll reports
/// nothing - it reads the current run, never the project's history - while `rigger validate`, the
/// whole-store order-signature detector, still reports that disorder. Store integrity is judged
/// over the run a poll reads; the history is `rigger validate`'s.
#[test]
fn watch_leaves_a_disorder_before_the_run_boundary_to_validate() {
    let dir = common::cli::temp_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init scaffolds the project: {err}");
    {
        let backend = Store::open(rigger_file(root, "events.db").to_str().unwrap()).unwrap();
        Namespaced::new(&backend, &run_stream_identity(root))
            .append(
                STREAM,
                ExpectedRevision::Any,
                &[
                    ev("RunNote", r#"{"id":"stale-0"}"#),
                    ev("RunNote", r#"{"id":"stale-1"}"#),
                    ev("RunNote", r#"{"id":"stale-2"}"#),
                    Event::new("RunStarted", br#"{"run":"run-c","criteria":[]}"#.to_vec())
                        .with_meta("run_id", "run-c"),
                    ev("RunNote", "{}"),
                ],
            )
            .unwrap();
    }
    // A stale writer swapped the first two history rows' revisions: the log's second row now
    // carries a revision below the first's, before the run's boundary.
    assert_eq!(
        rewrite_run_stream(root, "revision = 100", "revision = 0"),
        1
    );
    assert_eq!(rewrite_run_stream(root, "revision = 0", "revision = 1"), 1);
    assert_eq!(
        rewrite_run_stream(root, "revision = 1", "revision = 100"),
        1
    );

    assert_eq!(
        rigger_ok(root, &["watch", "--once"]),
        "",
        "a disorder before the boundary is not the poll's to report"
    );
    let (_out, err, ok) = run_rigger(root, &["validate"]);
    assert!(ok, "validate reports and exits 0: {err}");
    assert!(
        err.contains("stream run has 1 row(s)"),
        "validate still reports the disorder in the history: {err}"
    );

    // Control: a disorder INSIDE the run is the poll's to report - the poll judges the run it
    // reads, so its silence above is the boundary, not a blind poll.
    assert_eq!(
        rewrite_run_stream(root, "revision = 50", "type = 'RunStarted'"),
        1
    );
    let polled = rigger_ok(root, &["watch", "--once"]);
    assert!(
        polled.contains("store integrity") && polled.contains("1 row(s)"),
        "a disorder inside the run is reported: {polled}"
    );
}

/// A real ledger entry of `gc/src/a.rs` at `generation`, as the log carries one: its payload, its
/// identity as the group and its replay key.
fn ledger_entry(generation: &str) -> Event {
    generation_ingested("gc", "src/a.rs", generation, "b1", false).event(2)
}

/// A run stream holding perception on both sides of its boundary: a decision and a ledger entry,
/// then - when `run` names one - that run's `RunStarted`, then a note, a second ledger entry, a
/// derived edge and a second decision.
fn stream_with_ledger_entries(run: Option<&str>) -> Vec<Event> {
    let mut events = vec![
        ev(
            "DecisionMade",
            r#"{"id":"d-0","summary":"chose d-0","governs":["a.rs"]}"#,
        ),
        ledger_entry("h0"),
    ];
    events.extend(run.map(|run| run_started(run, &[])));
    events.extend([
        ev("RunNote", "{}"),
        ledger_entry("h1"),
        ev("EdgeInferred", r#"{"from":"a","rel":"CALLS","to":"b"}"#),
        ev(
            "DecisionMade",
            r#"{"id":"d-1","summary":"chose d-1","governs":["b.rs"]}"#,
        ),
    ]);
    events
}

/// Each event as the port hands it back: its type, its payload, its revision in its stream and
/// its position in the log.
fn answered(events: &[Event]) -> Vec<(&str, &str, i64, u64)> {
    events
        .iter()
        .map(|e| {
            (
                e.type_.as_str(),
                std::str::from_utf8(&e.data).unwrap(),
                e.revision,
                e.position,
            )
        })
        .collect()
}

/// Given one event list - a run with ledger entries and a derived edge on both sides of its
/// boundary - held by the sqlite store and by the hand-built log the domain's own tests read
/// through, when each is asked the two questions a read of the run asks (the boundary lookup and
/// the typed read), then the hand-built log answers exactly as the store does: the same boundary,
/// and the same events at the same revisions for the carried-over read from the start, the
/// perception-refusing read from the boundary, from the start and from one past the boundary, and
/// nothing of a stream it does not hold.
#[test]
fn the_hand_built_log_answers_the_boundary_lookup_and_the_typed_read_as_the_store_does() {
    let events = stream_with_ledger_entries(Some("run-p"));
    let store = Store::open(":memory:").unwrap();
    store
        .append(STREAM, ExpectedRevision::NoStream, &events)
        .unwrap();
    let log = HandBuiltLog::new(STREAM, events);

    assert_eq!(store.last_position(STREAM, "RunStarted").unwrap(), Some(2));
    assert_eq!(log.last_position(STREAM, "RunStarted").unwrap(), Some(2));
    assert_eq!(
        store.last_position(STREAM, "DecisionMade").unwrap(),
        Some(6)
    );
    assert_eq!(log.last_position(STREAM, "DecisionMade").unwrap(), Some(6));
    assert_eq!(store.last_position(STREAM, "UnitStarted").unwrap(), None);
    assert_eq!(log.last_position(STREAM, "UnitStarted").unwrap(), None);
    assert_eq!(log.last_position("another", "RunStarted").unwrap(), None);

    let both = |stream: &str, from: i64, selection: TypeSelection| {
        let stored = store.read_stream_typed(stream, from, selection).unwrap();
        let built = log.read_stream_typed(stream, from, selection).unwrap();
        assert_eq!(answered(&built), answered(&stored));
        answered(&stored)
            .into_iter()
            .map(|(type_, _, revision, _)| (type_.to_string(), revision))
            .collect::<Vec<_>>()
    };
    let pairs = |v: &[(&str, i64)]| -> Vec<(String, i64)> {
        v.iter().map(|(t, r)| (t.to_string(), *r)).collect()
    };
    assert_eq!(
        both(STREAM, 0, TypeSelection::Only(&CARRY_OVER)),
        pairs(&[("DecisionMade", 0), ("DecisionMade", 6)])
    );
    assert_eq!(
        both(STREAM, 2, TypeSelection::Except(&ONE_SHOT_PERCEPTION_TYPES)),
        pairs(&[("RunStarted", 2), ("RunNote", 3), ("DecisionMade", 6)])
    );
    assert_eq!(
        both(STREAM, 3, TypeSelection::Except(&ONE_SHOT_PERCEPTION_TYPES)),
        pairs(&[("RunNote", 3), ("DecisionMade", 6)])
    );
    assert_eq!(
        both(STREAM, 0, TypeSelection::Except(&ONE_SHOT_PERCEPTION_TYPES)),
        pairs(&[
            ("DecisionMade", 0),
            ("RunStarted", 2),
            ("RunNote", 3),
            ("DecisionMade", 6)
        ])
    );
    assert_eq!(
        both(STREAM, 0, TypeSelection::Only(&["GenerationIngested"])),
        pairs(&[("GenerationIngested", 1), ("GenerationIngested", 4)])
    );
    assert_eq!(
        both("another", 0, TypeSelection::Except(&[])),
        pairs(&[]),
        "a stream neither holds"
    );
}

/// Given one `events.db` file two projects share, each holding real ledger entries (payload, group
/// and replay key) and a derived edge in its run stream - `started` with a `RunStarted` between
/// its two entries, `unstarted` with none - when each project's current run is read through the
/// product's composition (a project namespace over the file-backed store), then no ledger entry
/// and no derived event is in either slice: the store is asked to refuse the five perception types
/// from the boundary (or from the start, where no run started) and materializes only what the run
/// read keeps, though the stream itself holds both entries.
#[test]
fn a_ledger_entry_in_the_run_stream_is_absent_from_the_current_run_read_through_the_namespace() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("events.db");
    let backend = Store::open(db.to_str().unwrap()).unwrap();
    let started = Namespaced::new(&backend, "started");
    let unstarted = Namespaced::new(&backend, "unstarted");
    started
        .append(
            STREAM,
            ExpectedRevision::NoStream,
            &stream_with_ledger_entries(Some("run-p")),
        )
        .unwrap();
    unstarted
        .append(
            STREAM,
            ExpectedRevision::NoStream,
            &stream_with_ledger_entries(None),
        )
        .unwrap();
    let perception = ONE_SHOT_PERCEPTION_TYPES.map(String::from).to_vec();
    let boundary_lookup = CountedRead::LastPosition {
        stream: STREAM.to_string(),
        event_type: "RunStarted".to_string(),
    };

    let counted = ReadCountingStore::new(&started);
    let (events, run_id) = rigger::run::read::read_current_run(&counted, STREAM).unwrap();
    assert_eq!(run_id, "run-p");
    assert_eq!(
        answered(&events)
            .iter()
            .map(|(t, _, r, _)| (*t, *r))
            .collect::<Vec<_>>(),
        [("RunStarted", 2), ("RunNote", 3), ("DecisionMade", 6)]
    );
    assert_eq!(payload_ids(&events), ["", "", "d-1"]);
    assert_eq!(
        counted.reads(),
        [
            boundary_lookup.clone(),
            CountedRead::Typed {
                stream: STREAM.to_string(),
                from: 0,
                only: true,
                types: CARRY_OVER.map(String::from).to_vec(),
                materialized: 2,
            },
            CountedRead::Typed {
                stream: STREAM.to_string(),
                from: 2,
                only: false,
                types: perception.clone(),
                materialized: 3,
            },
        ]
    );
    let whole = rigger::run::read::read_run(&started, STREAM).unwrap();
    assert_eq!(
        types_of(&whole),
        ["DecisionMade", "RunStarted", "RunNote", "DecisionMade"]
    );
    assert_eq!(payload_ids(&whole), ["d-0", "", "", "d-1"]);

    let counted = ReadCountingStore::new(&unstarted);
    let (events, run_id) = rigger::run::read::read_current_run(&counted, STREAM).unwrap();
    assert_eq!(run_id, "");
    assert_eq!(
        answered(&events)
            .iter()
            .map(|(t, _, r, _)| (*t, *r))
            .collect::<Vec<_>>(),
        [("DecisionMade", 0), ("RunNote", 2), ("DecisionMade", 5)]
    );
    assert_eq!(payload_ids(&events), ["d-0", "", "d-1"]);
    assert_eq!(
        counted.reads(),
        [
            boundary_lookup,
            CountedRead::Typed {
                stream: STREAM.to_string(),
                from: 0,
                only: false,
                types: perception,
                materialized: 3,
            },
        ]
    );

    // The entries are in each stream, group and replay key intact: the read refused them.
    for (project, revisions) in [(&started, [1, 4]), (&unstarted, [1, 3])] {
        let entries: Vec<(i64, Option<String>)> = project
            .read_stream(STREAM, 0, Direction::Forward)
            .unwrap()
            .into_iter()
            .filter(|e| e.type_ == "GenerationIngested")
            .map(|e| (e.revision, e.meta.get("replay_key").cloned()))
            .collect();
        assert_eq!(
            entries,
            [
                (revisions[0], Some("gc/src/a.rs@h0#2".to_string())),
                (revisions[1], Some("gc/src/a.rs@h1#2".to_string())),
            ]
        );
    }
}

/// Seed the repoless project at `root` with [`stream_with_ledger_entries`] under its own
/// namespace and make both ledger entries undecodable, so a command that materializes one fails -
/// as a whole-stream read of the seeded log now does.
fn seed_undecodable_ledger_entries(root: &Path, run: Option<&str>) {
    common::cli::seed_store(root);
    common::cli::write_workflow(root, "");
    let backend = Store::open(rigger_file(root, "events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    store
        .append(
            STREAM,
            ExpectedRevision::NoStream,
            &stream_with_ledger_entries(run),
        )
        .unwrap();
    assert_eq!(poison(root, "type = 'GenerationIngested'"), 2);
    assert_eq!(
        store
            .read_stream(STREAM, 0, Direction::Forward)
            .map(|events| events.len())
            .ok(),
        None,
        "the poison is live: a read that materializes a ledger entry fails"
    );
}

/// Given a project whose run stream holds a ledger entry on each side of the run's `RunStarted`,
/// both made undecodable, when the operator runs the one-shot commands and then drives the run
/// with `rigger step`, then each answers from the run and its carried-over decisions - a reader
/// that materialized a ledger entry would have failed on it.
#[test]
fn the_binary_answers_from_a_started_run_without_materializing_a_ledger_entry() {
    let dir = common::cli::temp_repoless_project();
    let root = dir.path();
    seed_undecodable_ledger_entries(root, Some("run-p"));

    assert_eq!(
        rigger_ok(root, &["peers"]),
        "decision d-0 | HISTORICAL | chose d-0 | governs: a.rs\n\
         decision d-1 | LIVE | chose d-1 | governs: b.rs\n"
    );
    assert_eq!(
        mcp_peers(root),
        json!({
            "decisions": [
                {"id": "d-0", "summary": "chose d-0", "governs": ["a.rs"], "live": false},
                {"id": "d-1", "summary": "chose d-1", "governs": ["b.rs"], "live": true},
            ],
            "lessons": [],
            "findings": [],
        })
    );
    assert_eq!(
        rigger_ok(root, &["prime"])
            .lines()
            .skip(1)
            .collect::<Vec<_>>(),
        [
            "# Rigger: recent decisions",
            "- d-1: chose d-1",
            "- d-0: chose d-0"
        ]
    );
    assert_eq!(
        rigger_ok(root, &["status"])
            .lines()
            .take(2)
            .collect::<Vec<_>>(),
        ["run run-p", "- . 0/0 units . healthy"]
    );
    assert_eq!(rigger_ok(root, &["watch", "--once"]), "");

    assert_eq!(
        rigger_ok(root, &["step"]),
        concat!(
            r#"{"wave":[{"id":"a/implementer#0","unit":"a","stage":"a","model":"sonnet","#,
            r#""tools":["Read","Edit"],"dir":"","max_wall_clock":null,"marker_path":null,"#,
            r#""cargo_target_dir":null}],"done":false}"#,
            "\n"
        ),
        "the first step adopts the run and parks the stage's spawn"
    );
    rigger_ok(root, &["result", "a/implementer#0", "done"]);
    assert_eq!(
        rigger_ok(root, &["step"]),
        "{\"wave\":[],\"done\":true}\n",
        "the second step replays the result and finishes the run"
    );
    assert_eq!(
        rigger_ok(root, &["status"])
            .lines()
            .take(2)
            .collect::<Vec<_>>(),
        ["run run-p", "a . 0/1 units . working"]
    );
}

/// Given a project whose run stream holds two undecodable ledger entries and NO `RunStarted`, so
/// the whole stream is the run, when the operator runs the one-shot commands and then `rigger
/// step`, then each answers from the stream's other events and the step starts a run over them - a
/// reader that materialized a ledger entry would have failed on it.
#[test]
fn the_binary_answers_from_an_unstarted_stream_without_materializing_a_ledger_entry() {
    let dir = common::cli::temp_repoless_project();
    let root = dir.path();
    seed_undecodable_ledger_entries(root, None);

    assert_eq!(
        rigger_ok(root, &["peers"]),
        "decision d-0 | LIVE | chose d-0 | governs: a.rs\n\
         decision d-1 | LIVE | chose d-1 | governs: b.rs\n",
        "with no run started the whole stream is the run, so both decisions are its own"
    );
    assert_eq!(
        rigger_ok(root, &["status"]).lines().next(),
        Some("- . 0/0 units . healthy"),
        "no run is named"
    );
    assert_eq!(rigger_ok(root, &["watch", "--once"]), "");

    assert_eq!(
        rigger_ok(root, &["step"]),
        concat!(
            r#"{"wave":[{"id":"a/implementer#0","unit":"a","stage":"a","model":"sonnet","#,
            r#""tools":["Read","Edit"],"dir":"","max_wall_clock":null,"marker_path":null,"#,
            r#""cargo_target_dir":null}],"done":false}"#,
            "\n"
        ),
        "the step starts a run over the stream and parks the stage's spawn"
    );
    assert_eq!(
        rigger_ok(root, &["status"]).lines().nth(1),
        Some("a . 0/1 units . working")
    );
    assert_eq!(
        rigger_ok(root, &["peers"]),
        "decision d-0 | HISTORICAL | chose d-0 | governs: a.rs\n\
         decision d-1 | HISTORICAL | chose d-1 | governs: b.rs\n",
        "the run the step started is after both decisions"
    );
}
