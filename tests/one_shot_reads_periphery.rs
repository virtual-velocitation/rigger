//! Periphery (contract / API / integration) tests for spec 101 criterion 2: ONE-SHOT COMMANDS READ
//! FROM THE BOUNDARY. `rigger status`, `rigger watch`, the dash snapshot, the side-car behind
//! `rigger peers` and the MCP tools read the run's own events from its boundary, with the derived
//! types excluded and the carried-over knowledge (decisions, lessons, findings) read by type. These
//! run OUTSIDE the crate and guard what the inside-out tests are structurally blind to:
//!
//!  - the in-crate tests count the read over a bare `:memory:` store; nothing drives the product's
//!    own composition - a project namespace over a file-backed `events.db` another project shares -
//!    and pins that the namespace forwards each typed read to the backend as ONE typed read of its
//!    own scoped stream, so the store (not the caller) refuses what the selection refuses;
//!  - the MCP tools used to hold a side-car subscription; nothing outside the crate pins that each
//!    call now reads afresh (a decision recorded between two calls is seen by the second, at the
//!    cost of exactly one more read) nor that the spawn-bound `rigger_progress` stamps the run the
//!    boundary read names;
//!  - no counting double reaches into the compiled binary, so the binary tests make any read past
//!    the slice observable instead: every derived event and every superseded run's own event in a
//!    real 200,000-event log is made undecodable, so a command that materialized even one of them
//!    would fail - and a control proves the poison does fail a command that reads it.

mod common;

use std::path::Path;
use std::process::Stdio;

use common::cli::{rigger_file, run_rigger, run_stream_identity, temp_store_project};
use common::fixtures::{ev, seed_one_shot_fixture, ONE_SHOT_DERIVED_TYPES};
use rigger::conductor::STREAM;
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore, ExpectedRevision};
use serde_json::{json, Value};

/// The carried-over types, spelled out: a run read hands back every one of them from every run.
const CARRY_OVER: [&str; 3] = ["DecisionMade", "LessonLearned", "ReviewFinding"];

/// `root`'s run stream as the compiled binary names it inside `events.db`.
fn scoped_run_stream(root: &Path) -> String {
    format!(
        "{}{STREAM}",
        Namespaced::prefix_for(&run_stream_identity(root))
    )
}

/// Make every event on `root`'s run stream that `condition` (an SQL predicate over the `events`
/// row) selects undecodable: its `meta` becomes a blob no read can turn back into an event, so any
/// command that materializes one fails. Returns how many it poisoned.
fn poison(root: &Path, condition: &str) -> usize {
    let conn = rusqlite::Connection::open(rigger_file(root, "events.db")).unwrap();
    conn.execute(
        &format!("UPDATE events SET meta = X'ff' WHERE stream = ?1 AND ({condition})"),
        [scoped_run_stream(root)],
    )
    .unwrap()
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
/// every derived event and every superseded run's own event undecodable - so a command that
/// materializes even one of them fails.
fn seed_poisoned_project(root: &Path, current: impl FnOnce(&dyn EventStore)) {
    let fixture = {
        let backend = Store::open(rigger_file(root, "events.db").to_str().unwrap()).unwrap();
        let store = Namespaced::new(&backend, &run_stream_identity(root));
        let fixture = seed_one_shot_fixture(&store, STREAM, &["the current campaign"]);
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
    let superseded = poison(
        root,
        &format!(
            "revision < {} AND type NOT IN ({}, {})",
            fixture.boundary,
            quoted(&CARRY_OVER),
            quoted(&ONE_SHOT_DERIVED_TYPES)
        ),
    );
    assert_eq!(
        superseded, 4,
        "both superseded runs' RunStarted and UnitStarted are poisoned"
    );
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
    seed_poisoned_project(root, |_| {});

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
    let reports = Namespaced::new(&progress_db, &run_stream_identity(root))
        .read_stream("progress/run-c", 0, Direction::Forward)
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
    assert_eq!(poison(root, "type = 'RunNote'"), 2);
    let (_out, _err, ok) = run_rigger(root, &["status"]);
    assert!(
        !ok,
        "a command that materializes a poisoned run event fails"
    );
}

/// Given the same poisoned project with a spawn parked and a unit escalated in the current run,
/// when a worker's couriers run - `rigger prompt`, `rigger scratch`, `rigger reported`, `rigger
/// hook stop-failure`, `rigger result` (plain and `--if-absent`) - and the operator runs `rigger
/// resume-unit`, then each answers from the current run: a courier that read past the run's
/// slice would have materialized a poisoned event and failed.
#[test]
fn the_worker_couriers_answer_from_the_run_without_materializing_a_derived_or_superseded_event() {
    let dir = temp_store_project();
    let root = dir.path();
    let spawn = "u/implementer#0";
    seed_poisoned_project(root, |store| {
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
    let rerecorded = rigger_ok(root, &["result", spawn, "done again"]);
    assert!(
        rerecorded.starts_with("recorded result for u/implementer#0"),
        "{rerecorded}"
    );
    let resumed = rigger_ok(root, &["resume-unit", "uc"]);
    assert!(resumed.starts_with("resumed unit \"uc\""), "{resumed}");
}
