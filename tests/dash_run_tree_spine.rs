//! Periphery (contract / API / integration) tests for the run-tree SPINE the dash projects
//! (spec 30, criterion c3): the run rendered as `spec -> unit -> stage -> role -> agent`, with
//! collapse/expand hints, driver-collapsed steps, and each node's live status.
//!
//! These run OUTSIDE the crate, over the library's PUBLIC surface (`rigger::dash`), so they guard
//! the boundary the inside-out unit test in `dash.rs` is structurally blind to. The implementer's
//! inside-out test drives `build_state` in-process on the happy path (one integrated unit + one
//! running unit under one spec) and greps the emitted JSON string. It never crosses the real HTTP
//! socket the operator's browser hits, never pins the `TreeNode` serialization contract that the
//! client render in `dash.html` depends on, and never exercises the projection's OTHER status arms
//! (a FAILED agent) or its grouping across MORE THAN ONE spec. This file adds exactly that layer:
//!
//!   1. The tree DTO crosses the real `/api/state` socket via the public `serve` entrypoint - the
//!      whole projection + `TreeNode` serialization end-to-end, not just the in-process builder.
//!   2. The `doing` field's `skip_serializing_if` contract: ABSENT (not null) when there is no
//!      live line, PRESENT when a running agent has one - the exact shape the client keys on.
//!   3. A FAILED agent leaf reads `failed` and rolls that failure up its role and stage.
//!   4. Units group by their id's spec prefix ACROSS specs, with the no-spec-number generic
//!      bucket, in the deterministic sorted order the client renders.
//!   5. The FAILURE / LIVENESS path, which no test drove and where four defects hid: a re-parked
//!      liveness fault reads `running` (a superseding OK reads `done`, via last-write-wins); an
//!      escalated unit renders Gates:`failed` and rolls that failure up to the spec root; a
//!      Gap-18 retry respawn renders as a DISTINCT sibling of its original; an in-flight unit's
//!      node status IS the shared blocker classification (reused, not re-derived); and the
//!      no-blocker sentinel falls back to the ledger status.
//!
//! Everything used here (`dash`, `spawn`, `progress`, `ledger`, `contextgraph`) is compiled on
//! BOTH the default and the `--no-default-features` lane - none is feature-gated - so these guard
//! the boundary in both lanes.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use rigger::conductor::META_REPLAY_KEY;
use rigger::contextgraph::{Graph, TYPE_GATE_VERDICT};
use rigger::dash::{self, DashInputs};
use rigger::eventstore::Event;
use rigger::progress::{AgentProgress, TYPE_AGENT_PROGRESS};
use rigger::spawn::{
    spawn_retry_id, SpawnRequest, SpawnResult, ROLE_ADJUDICATOR, ROLE_IMPLEMENTER,
};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Fixtures: build the exact serialized events a real run folds, over the public API only.
// ---------------------------------------------------------------------------

/// One event of `type_` with a JSON body, positioned by the caller via [`positioned`].
fn ev(type_: &str, json: &str) -> Event {
    Event::new(type_, json.as_bytes().to_vec())
}

/// Stamp 1-based stream positions the way the store would, so the snapshot's cursor and any
/// position-ordered fold see a realistic monotonic stream.
fn positioned(mut events: Vec<Event>) -> Vec<Event> {
    for (i, e) in events.iter_mut().enumerate() {
        e.position = (i + 1) as u64;
    }
    events
}

/// A recorded SUCCESS result answering a spawn (so its agent leaf reads `done`, not `running`).
fn ok_result(req: &SpawnRequest) -> Event {
    SpawnResult::ok(req.id.clone(), "ok").to_event().unwrap()
}

/// A recorded ERROR result answering `id` (so its agent leaf reads `failed`).
fn failed_result(id: &str, why: &str) -> Event {
    SpawnResult::failed(id.to_string(), why.to_string())
        .to_event()
        .unwrap()
}

/// A recorded gate-RUN verdict for `unit` at `attempt` - the SAME wire shape the conductor
/// records: a `GateVerdict` event whose replay-key metadata keys it to the unit's gate run
/// (`{unit}/gate:{gate}#{attempt}`, the conductor's gate-run key grammar). The projection reads
/// the unit's REAL gate outcome from these recorded verdicts, never inferred from ledger status.
fn gate_verdict(unit: &str, gate: &str, attempt: u32, pass: bool) -> Event {
    ev(
        TYPE_GATE_VERDICT,
        &format!(r#"{{"gate":"{gate}","pass":{pass},"flaky":false,"evidence":""}}"#),
    )
    .with_meta(META_REPLAY_KEY, format!("{unit}/gate:{gate}#{attempt}"))
}

/// A step-synthesized LIVENESS fault for `id` (spec 10): the driver re-parks such a result, so
/// the agent stays RUNNING and a later real result supersedes it (last-write-wins).
fn liveness_fault(id: &str) -> Event {
    SpawnResult::liveness_fault(id.to_string(), "no heartbeat within bound", "infra")
        .to_event()
        .unwrap()
}

/// A live progress ("doing") report for a spawn - the courier line the tree folds onto a
/// running agent so the spine subsumes the old live-agent-activity panel.
fn progress_event(id: &str, doing: &str) -> Event {
    let ap = AgentProgress {
        id: id.to_string(),
        activity: doing.to_string(),
    };
    Event::new(TYPE_AGENT_PROGRESS, serde_json::to_vec(&ap).unwrap())
}

/// The `StateView` as it goes on the wire: build it over the public API and serialize it to the
/// same JSON the `/api/state` endpoint emits and the client parses.
fn state_json(events: &[Event], progress: &[Event], liveness: &HashMap<String, u64>) -> Value {
    let state = dash::build_state(events, &Graph::default(), false, progress, liveness, 3)
        .expect("build_state projects the seeded run");
    serde_json::to_value(&state).expect("the state view serializes")
}

/// The child node labeled `label` under `node` (panics naming the miss, so a broken spine fails
/// loudly rather than silently navigating to `null`).
fn child<'a>(node: &'a Value, label: &str) -> &'a Value {
    node["children"]
        .as_array()
        .unwrap_or_else(|| panic!("node {} has no children array", node["label"]))
        .iter()
        .find(|c| c["label"] == label)
        .unwrap_or_else(|| panic!("no child labeled {label} under {}", node["label"]))
}

// ---------------------------------------------------------------------------
// 1. API: the tree DTO crosses the real HTTP /api/state boundary via `serve`.
// ---------------------------------------------------------------------------

/// Drive the hand-rolled dash server over a REAL loopback socket through the public `serve`
/// entrypoint and assert the run-tree spine comes back on `/api/state` with the right nesting,
/// the running unit's auto-expand path, and its live agent's doing-line - proving the projection
/// AND the `TreeNode` serialization survive the wire the browser actually reads, not just the
/// in-process builder the inside-out test exercises.
#[test]
fn run_tree_spine_crosses_the_http_state_boundary() {
    let a_impl = SpawnRequest::new("u30-c1", "implement", ROLE_IMPLEMENTER, 0, "impl A");
    let b_impl = SpawnRequest::new("u30-c2", "implement", ROLE_IMPLEMENTER, 0, "impl B");

    // Unit A (u30-c1): fully integrated. Unit B (u30-c2): implementer parked with NO result yet.
    let events = positioned(vec![
        ev(
            "UnitStarted",
            r#"{"id":"u30-c1","spec_criterion":"the shell"}"#,
        ),
        a_impl.to_event().unwrap(),
        ok_result(&a_impl),
        ev("UnitStatus", r#"{"id":"u30-c1","status":"green"}"#),
        ev("UnitStatus", r#"{"id":"u30-c1","status":"verified"}"#),
        ev("UnitStatus", r#"{"id":"u30-c1","status":"reviewed"}"#),
        ev("UnitIntegrated", r#"{"id":"u30-c1","commit":"abc"}"#),
        ev(
            "UnitStarted",
            r#"{"id":"u30-c2","spec_criterion":"the cells"}"#,
        ),
        b_impl.to_event().unwrap(),
    ]);
    let progress = vec![progress_event(&b_impl.id, "grep #7: dash.rs")];
    let liveness = HashMap::from([(b_impl.id.clone(), 5u64)]);

    // The provider `serve` re-reads per request. `Fn`, `Send`, `'static`: it owns and clones the
    // seeded inputs, exactly the shape `cmd_dash`'s store-reading provider yields.
    let provider = move || -> Result<DashInputs, String> {
        Ok((
            events.clone(),
            Graph::default(),
            progress.clone(),
            liveness.clone(),
        ))
    };

    // A free loopback port: bind an ephemeral listener, learn its port, release it, then serve
    // there (the same ephemeral-probe pattern the dash's own `free_port_from` uses).
    let port = TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    // A detached server thread: `serve` loops until the process ends; we drive one request.
    std::thread::spawn(move || {
        let _ = dash::serve(addr, provider, 3);
    });

    let mut client = connect_with_retry(addr);
    client
        .write_all(b"GET /api/state HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .unwrap();
    let mut resp = String::new();
    client.read_to_string(&mut resp).unwrap();

    assert!(
        resp.starts_with("HTTP/1.1 200 OK"),
        "the state endpoint returns 200:\n{resp}"
    );
    assert!(
        resp.contains("application/json"),
        "the state body is JSON:\n{resp}"
    );
    let body = resp.split("\r\n\r\n").nth(1).expect("a response body");
    let v: Value = serde_json::from_str(body).expect("the body parses as JSON");

    // The tree spine ships as a top-level array with one spec root grouping both units.
    let tree = v["tree"].as_array().expect("tree is a JSON array");
    assert_eq!(tree.len(), 1, "both units nest under one spec root");
    let spec = &tree[0];
    assert_eq!(spec["kind"], "spec");
    assert_eq!(spec["label"], "spec 30");
    assert_eq!(
        spec["children"].as_array().unwrap().len(),
        2,
        "spec 30 carries both units over the wire"
    );

    // The in-flight unit's whole path auto-expands, down to a RUNNING agent that carries its
    // live doing-line (the tree subsumes the activity panel, and it crosses the socket).
    let unit_b = child(spec, "u30-c2");
    assert_eq!(
        unit_b["auto_expand"], true,
        "the in-flight unit auto-expands"
    );
    let b_agent = child(
        child(child(unit_b, "Implement"), "implementer"),
        "attempt#0",
    );
    assert_eq!(b_agent["kind"], "agent");
    assert_eq!(b_agent["status"], "running", "the parked spawn is live");
    assert_eq!(
        b_agent["doing"], "grep #7: dash.rs",
        "the running agent's live doing-line crosses the socket"
    );

    // The integrated unit is NOT on the running path, and its finished agent carries NO doing key.
    let unit_a = child(spec, "u30-c1");
    assert_eq!(unit_a["status"], "integrated", "a node carries live status");
    assert_eq!(
        unit_a["auto_expand"], false,
        "a done unit is not auto-expanded"
    );
    let a_agent = child(
        child(child(unit_a, "Implement"), "implementer"),
        "attempt#0",
    );
    assert!(
        a_agent.get("doing").is_none(),
        "a finished agent omits the doing field entirely over the wire, got: {a_agent}"
    );
}

/// Connect to `addr`, retrying briefly while the detached `serve` thread finishes binding.
fn connect_with_retry(addr: SocketAddr) -> TcpStream {
    for _ in 0..200 {
        if let Ok(s) = TcpStream::connect(addr) {
            return s;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("the dash server never became reachable on {addr}");
}

// ---------------------------------------------------------------------------
// 2. Contract: the `doing` field's skip-when-absent serialization shape.
// ---------------------------------------------------------------------------

/// The `TreeNode.doing` field uses `skip_serializing_if = "Option::is_none"`, so the client
/// render in `dash.html` may key on its PRESENCE. This pins that serialization contract on the
/// actual wire JSON: a running agent WITH a live line carries `doing`; a running agent with NO
/// reported line omits the key entirely (absent, never a JSON `null`).
#[test]
fn tree_node_doing_is_omitted_when_absent_and_carried_when_live() {
    let loud = SpawnRequest::new("u30-c1", "implement", ROLE_IMPLEMENTER, 0, "loud");
    let quiet = SpawnRequest::new("u30-c2", "implement", ROLE_IMPLEMENTER, 0, "quiet");

    // Both implementers are parked (running); only `loud` has a live progress report.
    let events = positioned(vec![
        ev("UnitStarted", r#"{"id":"u30-c1"}"#),
        loud.to_event().unwrap(),
        ev("UnitStarted", r#"{"id":"u30-c2"}"#),
        quiet.to_event().unwrap(),
    ]);
    let progress = vec![progress_event(&loud.id, "reading conductor.rs")];
    let liveness = HashMap::from([(loud.id.clone(), 3u64), (quiet.id.clone(), 4u64)]);

    let v = state_json(&events, &progress, &liveness);
    let spec = &v["tree"][0];

    let loud_agent = child(
        child(child(child(spec, "u30-c1"), "Implement"), "implementer"),
        "attempt#0",
    );
    assert_eq!(
        loud_agent["status"], "running",
        "the reporting agent is running"
    );
    assert_eq!(
        loud_agent["doing"], "reading conductor.rs",
        "a live agent carries its doing-line"
    );

    let quiet_agent = child(
        child(child(child(spec, "u30-c2"), "Implement"), "implementer"),
        "attempt#0",
    );
    assert_eq!(
        quiet_agent["status"], "running",
        "the silent agent is also running"
    );
    assert!(
        quiet_agent.get("doing").is_none(),
        "an agent with no reported line omits `doing` entirely (not null): {quiet_agent}"
    );
}

// ---------------------------------------------------------------------------
// 3. Integration: a FAILED agent reads `failed` and rolls that failure up.
// ---------------------------------------------------------------------------

/// A spawn answered with an ERROR reads `failed` at its leaf, and that failure rolls up the role
/// and the stage above it. The inside-out test only exercises `done` and `running` leaves, so
/// this guards the `errored` arm of the projection and the `failed` branch of the status rollup.
#[test]
fn run_tree_reads_a_failed_agent_and_rolls_failure_up() {
    let a_impl = SpawnRequest::new("u30-c1", "implement", ROLE_IMPLEMENTER, 0, "impl A");
    let failed = SpawnResult::failed(a_impl.id.clone(), "the build did not compile")
        .to_event()
        .unwrap();

    let events = positioned(vec![
        ev("UnitStarted", r#"{"id":"u30-c1"}"#),
        a_impl.to_event().unwrap(),
        failed,
    ]);

    let v = state_json(&events, &[], &HashMap::new());
    let spec = &v["tree"][0];
    let implement = child(child(spec, "u30-c1"), "Implement");

    assert_eq!(
        implement["status"], "failed",
        "the stage rolls up the failed agent"
    );
    let role = child(implement, "implementer");
    assert_eq!(
        role["status"], "failed",
        "the role rolls up the failed agent"
    );
    let agent = child(role, "attempt#0");
    assert_eq!(
        agent["status"], "failed",
        "an errored spawn's leaf reads failed, not done"
    );
    assert_eq!(
        agent["auto_expand"], false,
        "a failed leaf is not live/running"
    );
}

/// A CRASHED implementer must NOT render a Gates node. `errored` is a subset of `answered` (a
/// failed result answers the spawn), so gating the Gates-node presence on merely "an implementer
/// answered" renders a Gates node for a unit whose implementer FAILED - and with no gate verdict
/// yet it reads `running`, so the spine shows Implement:failed + Gates:running under a building
/// unit: a gate line that claims to be running when no gate ran or can run (no gate runs without a
/// SUCCESSFUL implementer). The Gates node is present only for a SUCCESSFUL implementer
/// (`answered && !errored`) or a unit already past green, so a crashed implementer surfaces its
/// failure at Implement and shows no phantom Gates line.
#[test]
fn a_crashed_implementer_renders_no_gates_node() {
    let impl0 = SpawnRequest::new("u30-c1", "implement", ROLE_IMPLEMENTER, 0, "attempt 0");
    let events = positioned(vec![
        ev("UnitStarted", r#"{"id":"u30-c1"}"#),
        impl0.to_event().unwrap(),
        // The implementer CRASHED (an error result), and no gate has run - the unit is still
        // pre-green (grounding), so the Gates node has no basis to render.
        failed_result(&impl0.id, "the implementer process exited non-zero"),
    ]);

    let v = state_json(&events, &[], &HashMap::new());
    let unit = child(&v["tree"][0], "u30-c1");

    // The Implement stage carries the crash; the Gates stage is ABSENT (no gate ran or can run).
    assert_eq!(
        child(unit, "Implement")["status"],
        "failed",
        "the crashed implementer surfaces its failure at the Implement stage"
    );
    let stage_labels: Vec<&str> = unit["children"]
        .as_array()
        .expect("the unit has a children array")
        .iter()
        .map(|c| c["label"].as_str().unwrap())
        .collect();
    assert!(
        !stage_labels.contains(&"Gates"),
        "a crashed implementer renders no Gates node (no gate ran without a successful implementer): {stage_labels:?}"
    );
}

// ---------------------------------------------------------------------------
// 4. Integration: grouping by spec prefix across specs, plus the generic bucket.
// ---------------------------------------------------------------------------

/// Units group under one root per spec (their id's leading spec number), with a generic bucket
/// for an id that carries no spec number, in the deterministic sorted order the client renders.
/// The inside-out test only has one spec (spec 30); this guards `spec_of`'s cross-spec grouping
/// and its no-spec-number fallback at the projection boundary.
#[test]
fn run_tree_groups_units_by_spec_and_falls_back_to_a_generic_bucket() {
    let events = positioned(vec![
        ev("UnitStarted", r#"{"id":"u30-c1"}"#),
        ev("UnitStarted", r#"{"id":"u28-3"}"#),
        ev("UnitStarted", r#"{"id":"misc-thing"}"#),
    ]);

    let v = state_json(&events, &[], &HashMap::new());
    let roots = v["tree"].as_array().expect("tree is an array");

    // One root per spec, deterministically sorted: the generic bucket "spec" sorts before the
    // numbered "spec 28" and "spec 30".
    let labels: Vec<&str> = roots.iter().map(|r| r["label"].as_str().unwrap()).collect();
    assert_eq!(
        labels,
        vec!["spec", "spec 28", "spec 30"],
        "units group into one deterministically-ordered root per spec, generic bucket first"
    );
    assert!(
        roots.iter().all(|r| r["kind"] == "spec"),
        "every root is a spec node"
    );

    // Each unit lands under the right root: the spec-numbered ids under their number, the
    // no-spec-number id under the generic bucket.
    let spec30 = roots.iter().find(|r| r["label"] == "spec 30").unwrap();
    child(spec30, "u30-c1");
    let spec28 = roots.iter().find(|r| r["label"] == "spec 28").unwrap();
    child(spec28, "u28-3");
    let generic = roots.iter().find(|r| r["label"] == "spec").unwrap();
    child(generic, "misc-thing");
}

// ---------------------------------------------------------------------------
// 5. Liveness path: a re-parked liveness fault is RUNNING; a superseding OK is DONE.
// ---------------------------------------------------------------------------

/// The projection reads answered/errored PER SPAWN from the typed last-write-wins authority,
/// never a union over every recorded `SpawnResult`. So (a) an agent whose LATEST result is a
/// success reads `done` even though an EARLIER liveness fault carries a non-empty error, and
/// (b) an agent whose only result is a step-synthesized liveness fault reads `running` (the
/// driver re-parked it), NOT `failed`. Neither path is driven by the inside-out test; without
/// this a still-hung or hung-then-recovered agent would render a FALSE failure and roll it up.
#[test]
fn a_re_parked_liveness_fault_reads_running_and_a_superseding_ok_reads_done() {
    let recovered = SpawnRequest::new("u30-c1", "implement", ROLE_IMPLEMENTER, 0, "recovered");
    let still_hung = SpawnRequest::new("u30-c2", "implement", ROLE_IMPLEMENTER, 0, "still hung");

    // u30-c1: a hung agent's liveness fault, THEN a real success answers the same spawn.
    // u30-c2: a hung agent with ONLY the liveness fault - re-parked, awaiting a real result.
    let events = positioned(vec![
        ev("UnitStarted", r#"{"id":"u30-c1"}"#),
        recovered.to_event().unwrap(),
        liveness_fault(&recovered.id),
        ok_result(&recovered),
        ev("UnitStarted", r#"{"id":"u30-c2"}"#),
        still_hung.to_event().unwrap(),
        liveness_fault(&still_hung.id),
    ]);

    let v = state_json(&events, &[], &HashMap::new());
    let spec = &v["tree"][0];

    // (a) The recovered agent: its newest result wins, so it reads done - not the stale fault.
    let recovered_agent = child(
        child(child(child(spec, "u30-c1"), "Implement"), "implementer"),
        "attempt#0",
    );
    assert_eq!(
        recovered_agent["status"], "done",
        "a hung-then-recovered agent reads done via last-write-wins, not a false failure"
    );
    assert_eq!(
        recovered_agent["auto_expand"], false,
        "a recovered (done) agent is not on the live path"
    );

    // (b) The still-hung agent: the re-parked fault is not an answer, so it reads running.
    let hung_agent = child(
        child(child(child(spec, "u30-c2"), "Implement"), "implementer"),
        "attempt#0",
    );
    assert_eq!(
        hung_agent["status"], "running",
        "a re-parked liveness fault reads running (the driver treats it as live), never failed"
    );
    assert_eq!(
        child(spec, "u30-c2")["auto_expand"],
        true,
        "the still-hung unit is on the auto-expand live path"
    );
}

// ---------------------------------------------------------------------------
// 6. Failure path: an escalated unit renders Gates:failed and rolls up to the spec root.
// ---------------------------------------------------------------------------

/// A unit that RED-failed its gates and ESCALATED must not mask the failure: the Gates driver
/// line carries the unit's REAL gate outcome (`failed`, never a hardcoded `passed`), the unit
/// node carries `escalated`, and that terminal failure rolls all the way up to the spec root
/// (which previously only ever yielded running/integrated/building, so a dead unit rendered
/// `building` forever). None of these arms is driven by the inside-out test.
#[test]
fn an_escalated_unit_renders_gates_failed_and_surfaces_at_the_spec_root() {
    let impl0 = SpawnRequest::new("u30-c1", "implement", ROLE_IMPLEMENTER, 0, "attempt 0");

    let events = positioned(vec![
        ev("UnitStarted", r#"{"id":"u30-c1"}"#),
        impl0.to_event().unwrap(),
        failed_result(&impl0.id, "gates: cargo test FAILED"),
        // The gate itself recorded a FAILING verdict - the REAL gate outcome the Gates node
        // reads, so a gate failure surfaces from the recorded verdict (not from ledger status).
        gate_verdict("u30-c1", "test", 0, false),
        ev("UnitStatus", r#"{"id":"u30-c1","status":"red"}"#),
        ev("UnitEscalated", r#"{"id":"u30-c1"}"#),
    ]);

    let v = state_json(&events, &[], &HashMap::new());
    let spec = &v["tree"][0];

    // The unit node carries its terminal live status.
    let unit = child(spec, "u30-c1");
    assert_eq!(
        unit["status"], "escalated",
        "the escalated unit carries its live status"
    );

    // The Gates driver line - the only place a driver-run gate failure surfaces - reads failed.
    let gates = child(unit, "Gates");
    assert_eq!(
        gates["status"], "failed",
        "a gate-failed/escalated unit renders Gates:failed, never a hardcoded passed"
    );
    assert_eq!(
        gates["children"][0]["status"], "failed",
        "the collapsed driver line under Gates is failed too"
    );

    // The failure rolls up to the spec root (was masked as building forever).
    assert_eq!(
        spec["status"], "escalated",
        "an escalated child surfaces at the spec root, not a perpetual building"
    );
}

/// The FULL implement-gate battery the real stage runs, not a lone `test=false` fixture. `run_gates`
/// iterates [fmt, clippy, build, test, style] with NO break on failure, so a `test` FAILURE is
/// followed by `style` passing LAST at the SAME attempt. A last-write-wins-by-unit fold of the
/// recorded verdicts would return that trailing `style=true` and render Gates:PASSED for a unit
/// whose gates FAILED - the exact mask a single-gate fixture cannot exercise. The Gates node must
/// read the AND across the attempt's gates, so a masked `test` failure still surfaces as failed and
/// rolls up to the spec root. This fixture makes the mask impossible to re-green.
#[test]
fn an_escalated_units_gate_failure_is_not_masked_by_a_trailing_passing_gate() {
    let impl0 = SpawnRequest::new("u30-c1", "implement", ROLE_IMPLEMENTER, 0, "attempt 0");

    let events = positioned(vec![
        ev("UnitStarted", r#"{"id":"u30-c1"}"#),
        impl0.to_event().unwrap(),
        // The implementer SUCCEEDED (it wrote code); the gate battery is what failed.
        ok_result(&impl0),
        // The real 5-gate implement battery at attempt 0, in run order: `test` FAILS, then
        // `style` passes LAST - the trailing pass a last-write-wins fold would mistake for the
        // unit's outcome.
        gate_verdict("u30-c1", "fmt", 0, true),
        gate_verdict("u30-c1", "clippy", 0, true),
        gate_verdict("u30-c1", "build", 0, true),
        gate_verdict("u30-c1", "test", 0, false),
        gate_verdict("u30-c1", "style", 0, true),
        ev("UnitStatus", r#"{"id":"u30-c1","status":"red"}"#),
        ev("UnitEscalated", r#"{"id":"u30-c1"}"#),
    ]);

    let v = state_json(&events, &[], &HashMap::new());
    let spec = &v["tree"][0];
    let unit = child(spec, "u30-c1");

    // The Gates node reads the AND across the attempt's gates: the `test` failure is NOT masked by
    // the trailing `style=true`, so it renders failed - never a passed the operator would trust.
    let gates = child(unit, "Gates");
    assert_ne!(
        gates["status"], "passed",
        "a failing test gate must not be masked by a trailing passing style gate (last-write-wins mask)"
    );
    assert_eq!(
        gates["status"], "failed",
        "the Gates node reads any-gate-failed across the whole battery, not the last verdict logged"
    );
    // And the masked failure still surfaces at the spec root.
    assert_eq!(
        spec["status"], "escalated",
        "the unmasked gate failure rolls up to the spec root"
    );
}

/// The OFF-LINEAR terminals (`Failed` / `Escalated`) must not fabricate a phantom `Gates:passed`.
/// `status_rank` aliases them to `Green`'s rank (both are "past implement"), but they reached that
/// rank by FAILING, not by clearing the gates: a crash-to-exhaustion unit escalates with its
/// implementer crashed every attempt and the gate block SKIPPED (the driver short-circuits it on
/// `spawn_err`), so NO gate ever ran and NO verdict was recorded. Inferring a gate PASS from that
/// aliased rank invents an outcome the gates never produced - the same fabricate-a-gate-outcome-
/// from-status defect the Gates node exists to kill. Two off-linear triggers, BOTH with a crashed
/// implementer and NO recorded gate verdict, driven end-to-end through the `/api/state` DTO:
/// (u30-c1) a terminal ESCALATED unit, and (u30-c2) a mid-remediation `Failed` (`reject-recurrence`,
/// NOT yet escalated - LIVE during the run, so the mask is not a self-correcting terminal-only
/// transient). Neither has a successful implementer or a recorded verdict, so no gate ran or can:
/// each renders NO Gates node at all (and thus can never render `Gates:passed`) and surfaces its
/// failure at Implement. Reverting the off-linear presence guard re-aliases them to `Green` and the
/// phantom `Gates:passed` returns, reddening this test.
#[test]
fn an_off_linear_unit_with_no_gate_verdict_renders_no_phantom_gates_passed() {
    let esc_impl = SpawnRequest::new("u30-c1", "implement", ROLE_IMPLEMENTER, 0, "crash");
    let fail_impl = SpawnRequest::new("u30-c2", "implement", ROLE_IMPLEMENTER, 0, "crash");

    let events = positioned(vec![
        // u30-c1: crash-to-exhaustion. The implementer crashed (an error result), the gate block was
        // skipped (no gate ran, NO verdict recorded), and the unit ESCALATED.
        ev("UnitStarted", r#"{"id":"u30-c1"}"#),
        esc_impl.to_event().unwrap(),
        failed_result(
            &esc_impl.id,
            "the implementer process crashed to exhaustion",
        ),
        ev("UnitEscalated", r#"{"id":"u30-c1"}"#),
        // u30-c2: a mid-remediation crash. The implementer crashed and the unit was marked `Failed`
        // (reject-recurrence) with NO gate verdict - live during the run, not yet escalated.
        ev("UnitStarted", r#"{"id":"u30-c2"}"#),
        fail_impl.to_event().unwrap(),
        failed_result(&fail_impl.id, "the remediation implementer crashed"),
        ev("UnitFailed", r#"{"id":"u30-c2","attempts":1}"#),
    ]);

    let v = state_json(&events, &[], &HashMap::new());
    let spec = &v["tree"][0];

    // Both off-linear terminals: each carries its REAL live status and renders NO phantom Gates line.
    for (unit_id, want_status) in [("u30-c1", "escalated"), ("u30-c2", "reject-recurrence")] {
        let unit = child(spec, unit_id);
        assert_eq!(
            unit["status"], want_status,
            "{unit_id} carries its real off-linear status, not a masked one"
        );
        // The crash surfaces at the Implement stage...
        assert_eq!(
            child(unit, "Implement")["status"],
            "failed",
            "{unit_id}'s crashed implementer surfaces its failure at the Implement stage"
        );
        // ...and there is NO Gates node: no gate ran (crashed implementer, no recorded verdict), so
        // the spine cannot fabricate a `passed` for gates that never ran.
        let stage_labels: Vec<&str> = unit["children"]
            .as_array()
            .expect("the unit has a children array")
            .iter()
            .map(|c| c["label"].as_str().unwrap())
            .collect();
        assert!(
            !stage_labels.contains(&"Gates"),
            "{unit_id} (off-linear, no recorded verdict) renders NO Gates node - no phantom passed: {stage_labels:?}"
        );
    }
}

/// The other direction of the any-gate-failed fold: a unit that FAILED its gate battery at one
/// attempt then RE-GATED GREEN at the next must render Gates:`passed`, not carry the earlier red
/// forever. `recorded_gate_outcome` restricts the AND to the LATEST attempt (distinct attempts are
/// distinct gate runs, keyed by `#{attempt}`), so attempt 0's `test=false` does NOT poison
/// attempt 1's all-green battery. Without that restriction (an AND across ALL attempts) a recovered
/// unit would render Gates:`failed` after it had already re-gated green - the inverse mask. Pinned
/// end-to-end through the `/api/state` DTO so the recovery direction of the seam cannot regress:
/// the masked-failure test guards `Some(false)`, this guards the latest-attempt `Some(true)`.
#[test]
fn a_regated_green_unit_renders_gates_passed_despite_an_earlier_failed_attempt() {
    let impl0 = SpawnRequest::new("u30-c1", "implement", ROLE_IMPLEMENTER, 0, "attempt 0");
    let impl1 = SpawnRequest::new("u30-c1", "implement", ROLE_IMPLEMENTER, 1, "attempt 1");

    let events = positioned(vec![
        ev("UnitStarted", r#"{"id":"u30-c1"}"#),
        // Attempt 0: the implementer wrote code, but the gate battery FAILED on `test` (with
        // `style` passing LAST at the same attempt), so the unit went red.
        impl0.to_event().unwrap(),
        ok_result(&impl0),
        gate_verdict("u30-c1", "fmt", 0, true),
        gate_verdict("u30-c1", "clippy", 0, true),
        gate_verdict("u30-c1", "build", 0, true),
        gate_verdict("u30-c1", "test", 0, false),
        gate_verdict("u30-c1", "style", 0, true),
        ev("UnitStatus", r#"{"id":"u30-c1","status":"red"}"#),
        // Attempt 1: the retried implementer succeeded and the WHOLE battery passed, so the unit
        // re-gated green and integrated. The earlier attempt's red is a distinct, superseded run.
        impl1.to_event().unwrap(),
        ok_result(&impl1),
        gate_verdict("u30-c1", "fmt", 1, true),
        gate_verdict("u30-c1", "clippy", 1, true),
        gate_verdict("u30-c1", "build", 1, true),
        gate_verdict("u30-c1", "test", 1, true),
        gate_verdict("u30-c1", "style", 1, true),
        ev("UnitStatus", r#"{"id":"u30-c1","status":"green"}"#),
        ev("UnitStatus", r#"{"id":"u30-c1","status":"verified"}"#),
        ev("UnitStatus", r#"{"id":"u30-c1","status":"reviewed"}"#),
        ev("UnitIntegrated", r#"{"id":"u30-c1","commit":"def"}"#),
    ]);

    let v = state_json(&events, &[], &HashMap::new());
    let spec = &v["tree"][0];
    let unit = child(spec, "u30-c1");

    // The Gates node reads the LATEST attempt's AND: attempt 1's all-green battery, NOT attempt 0's
    // masked red. A regression ANDing across all attempts would render `failed` here.
    let gates = child(unit, "Gates");
    assert_eq!(
        gates["status"], "passed",
        "a re-gated-green unit reads Gates:passed - the latest attempt's outcome, not an earlier failed attempt's red"
    );
    // And the recovered unit surfaces as integrated at the unit and spec root, no lingering failure.
    assert_eq!(
        unit["status"], "integrated",
        "the recovered unit integrated"
    );
    assert_eq!(
        spec["status"], "integrated",
        "the recovered unit's spec root reads integrated, not a lingering failure"
    );
}

// ---------------------------------------------------------------------------
// 6a. Gate-outcome source: the Gates node reads the RECORDED gate verdict, not ledger status.
// ---------------------------------------------------------------------------

/// A unit that reached GREEN (its gates PASSED, recorded a passing verdict), was VERIFIED, then
/// got REVIEW-REJECTED (`UnitFailed` = reject-recurrence, mid-remediation) must NOT fabricate a
/// gate failure. Its gates never failed, so the Gates node reads `passed`; the reject is a
/// review/unit-level status and surfaces there (`reject-recurrence`), never masquerading as a
/// gate failure. Sourcing the Gates node from ledger status (`Failed`) would render Gates:failed
/// (a gate failure that never happened) and HIDE the reject behind it (Review shows done), so the
/// Gates node must read the RECORDED gate verdict, not `ledger::Status`.
#[test]
fn a_review_rejected_unit_whose_gates_passed_renders_gates_passed_and_surfaces_the_reject() {
    let impl0 = SpawnRequest::new("u30-c1", "implement", ROLE_IMPLEMENTER, 0, "attempt 0");
    let adj0 = SpawnRequest::new("u30-c1", "review", ROLE_ADJUDICATOR, 0, "adjudicator");

    let events = positioned(vec![
        ev("UnitStarted", r#"{"id":"u30-c1"}"#),
        impl0.to_event().unwrap(),
        ok_result(&impl0),
        // The unit's gates PASSED at attempt 0 - the recorded verdict is the REAL gate outcome.
        gate_verdict("u30-c1", "test", 0, true),
        ev("UnitStatus", r#"{"id":"u30-c1","status":"green"}"#),
        ev("UnitStatus", r#"{"id":"u30-c1","status":"verified"}"#),
        adj0.to_event().unwrap(),
        ok_result(&adj0),
        // The review REJECTED the (green, gates-passed) unit: it re-enters remediation.
        ev("UnitFailed", r#"{"id":"u30-c1","attempts":1}"#),
    ]);

    let v = state_json(&events, &[], &HashMap::new());
    let unit = child(&v["tree"][0], "u30-c1");

    // The Gates node reads the REAL (passing) gate outcome - a review reject is not a gate fail.
    let gates = child(unit, "Gates");
    assert_ne!(
        gates["status"], "failed",
        "a review reject must not fabricate a gate failure the gates never produced"
    );
    assert_eq!(
        gates["status"], "passed",
        "a review-rejected unit whose last gate run PASSED renders Gates:passed"
    );

    // The reject surfaces where it actually is - the unit's live status - so it is not invisible.
    assert_eq!(
        unit["status"], "reject-recurrence",
        "the review reject surfaces at the unit level as reject-recurrence, not on the Gates node"
    );
}

/// The BETWEEN-STEPS window: an implementer RESULT is recorded but the green `UnitStatus` has not
/// been emitted yet, so the unit is still `grounding` and NO gate has run (no recorded verdict).
/// The Gates node renders (the implementer answered), but the gates have not run - so it must NOT
/// read `failed`. Sourcing it from ledger status (`Grounding`) fabricated a gate failure before
/// any gate ran; sourcing it from the recorded verdict (absent) reads it as still running.
#[test]
fn a_pre_gate_unit_whose_implementer_finished_does_not_render_gates_failed() {
    use rigger::ledger;

    let impl0 = SpawnRequest::new("u30-c1", "implement", ROLE_IMPLEMENTER, 0, "attempt 0");
    let events = positioned(vec![
        ev("UnitStarted", r#"{"id":"u30-c1"}"#),
        impl0.to_event().unwrap(),
        // The implementer finished; gates have not run yet, so no gate verdict is recorded.
        ok_result(&impl0),
    ]);

    // Precondition: the unit is still grounding (no green UnitStatus emitted yet).
    let run = ledger::project(&events).expect("the run projects");
    assert_eq!(
        run.units["u30-c1"].status.as_str(),
        "grounding",
        "precondition: the implementer answered but the unit has not reached green"
    );

    let v = state_json(&events, &[], &HashMap::new());
    let unit = child(&v["tree"][0], "u30-c1");
    let gates = child(unit, "Gates");
    assert_ne!(
        gates["status"], "failed",
        "a pre-gate unit (implementer done, no gate run yet) must not fabricate Gates:failed"
    );
    assert_eq!(
        gates["status"], "running",
        "with no recorded gate verdict the Gates node reads running, never a failure before gates ran"
    );
}

/// The OTHER `None` (no recorded verdict) sub-case, distinct from the pre-gate window above: a
/// unit the ledger already advanced to green or beyond - which it does ONLY after its gates PASS -
/// whose event slice carries NO recorded gate verdict (a windowed / pruned slice, or a log from
/// before the verdict was recorded). Its gates are ALREADY CLEARED, so the Gates node must render
/// `passed` via the rank fallback, never regress to `running` as if the gates were still in flight
/// (nor `failed`). This pins the `None if rank >= Green => "passed"` arm: dropping it would render
/// every verdict-less integrated unit as Gates:running, misreporting a fully-landed unit as one
/// whose gates never finished - and every sibling gate-outcome test still passes without it.
#[test]
fn a_gates_cleared_unit_with_no_recorded_verdict_still_renders_gates_passed() {
    use rigger::ledger;

    let impl0 = SpawnRequest::new("u30-c1", "implement", ROLE_IMPLEMENTER, 0, "attempt 0");
    let events = positioned(vec![
        ev("UnitStarted", r#"{"id":"u30-c1"}"#),
        impl0.to_event().unwrap(),
        ok_result(&impl0),
        // The unit's gates cleared and it landed - but no gate verdict is present in this slice.
        ev("UnitStatus", r#"{"id":"u30-c1","status":"green"}"#),
        ev("UnitStatus", r#"{"id":"u30-c1","status":"integrated"}"#),
    ]);

    // Precondition: the unit is integrated (rank >= Green) and NO gate verdict is recorded, so
    // `recorded_gate_outcome` is `None` and only the rank fallback can decide passed-vs-running.
    let run = ledger::project(&events).expect("the run projects");
    assert_eq!(
        run.units["u30-c1"].status.as_str(),
        "integrated",
        "precondition: the unit landed (gates cleared) with no recorded verdict in the slice"
    );

    let v = state_json(&events, &[], &HashMap::new());
    let unit = child(&v["tree"][0], "u30-c1");
    let gates = child(unit, "Gates");
    assert_ne!(
        gates["status"], "running",
        "a gates-cleared (integrated) unit must not regress to running as if its gates never finished"
    );
    assert_ne!(
        gates["status"], "failed",
        "an integrated unit whose gates cleared has no failure to fabricate"
    );
    assert_eq!(
        gates["status"], "passed",
        "a green+ unit with no recorded verdict renders Gates:passed via the gates-already-cleared fallback"
    );
}

// ---------------------------------------------------------------------------
// 7. Remediation path: multi-attempt and Gap-18 retry spawns render as DISTINCT siblings.
// ---------------------------------------------------------------------------

/// A remediated unit is exactly what an operator opens the spine to inspect. Each implementer
/// attempt renders as its own agent (`attempt#0`, `attempt#1`), and a Gap-18 reviewer RESPAWN -
/// which SHARES its original's attempt ordinal but carries a `~retryN` suffix - renders as a
/// DISTINCT sibling (`attempt#0 retry2`), never collapsing into the original's identical label.
#[test]
fn multi_attempt_and_gap18_retry_spawns_render_as_distinct_sibling_agents() {
    let impl0 = SpawnRequest::new("u30-c1", "implement", ROLE_IMPLEMENTER, 0, "attempt 0");
    let impl1 = SpawnRequest::new("u30-c1", "implement", ROLE_IMPLEMENTER, 1, "attempt 1");

    // A degenerate adjudicator result (empty) triggers a Gap-18 respawn under a ~retry2 id that
    // shares the original's attempt ordinal 0.
    let adj0 = SpawnRequest::new("u30-c1", "review", ROLE_ADJUDICATOR, 0, "adj original");
    let mut adj0_retry = SpawnRequest::new("u30-c1", "review", ROLE_ADJUDICATOR, 0, "adj respawn");
    adj0_retry.id = spawn_retry_id("u30-c1", ROLE_ADJUDICATOR, 0, 2);
    assert_eq!(
        adj0_retry.id, "u30-c1/adjudicator#0~retry2",
        "precondition: the respawn shares attempt 0 and carries the ~retry2 suffix"
    );

    let events = positioned(vec![
        ev("UnitStarted", r#"{"id":"u30-c1"}"#),
        impl0.to_event().unwrap(),
        failed_result(&impl0.id, "attempt 0 failed"),
        impl1.to_event().unwrap(),
        ok_result(&impl1),
        adj0.to_event().unwrap(),
        ok_result(&adj0),
        adj0_retry.to_event().unwrap(),
        ok_result(&adj0_retry),
    ]);

    let v = state_json(&events, &[], &HashMap::new());
    let unit = child(&v["tree"][0], "u30-c1");

    // Two implementer attempts: two distinct sibling agents in ordinal order.
    let impl_role = child(child(unit, "Implement"), "implementer");
    let impl_labels: Vec<&str> = impl_role["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["label"].as_str().unwrap())
        .collect();
    assert_eq!(
        impl_labels,
        vec!["attempt#0", "attempt#1"],
        "each implementer attempt renders as its own distinct agent"
    );
    // The implementer role mixes a FAILED child (attempt#0) and a DONE child (attempt#1) with
    // nothing running, so it pins the rollup's failed-beats-done precedence: a recovered unit's
    // stage rolls up to `failed`, and a regression inverting that precedence would fail here.
    assert_eq!(
        impl_role["status"], "failed",
        "a role with a failed attempt and a done attempt rolls up to failed (failed beats done)"
    );

    // The adjudicator original and its Gap-18 respawn: two DISTINCT agents, never collapsed.
    let adj_role = child(child(unit, "Review"), "adjudicator");
    let adj_agents = adj_role["children"].as_array().unwrap();
    assert_eq!(
        adj_agents.len(),
        2,
        "the respawn does not collapse into its original (was one indistinguishable label)"
    );
    let adj_labels: Vec<&str> = adj_agents
        .iter()
        .map(|a| a["label"].as_str().unwrap())
        .collect();
    assert!(
        adj_labels.contains(&"attempt#0") && adj_labels.contains(&"attempt#0 retry2"),
        "the respawn carries a distinguishing retry marker: {adj_labels:?}"
    );
}

// ---------------------------------------------------------------------------
// 8. Reuse contract: an in-flight unit's node status IS the shared blocker classification.
// ---------------------------------------------------------------------------

/// The load-bearing reuse contract (the tree and `rigger status` cannot drift): an in-flight
/// unit's node status is the SAME `blocker::from_state` kind_tag the operator's status line
/// shows, reused - never re-derived. Asserted by classifying independently and comparing, so a
/// re-derive or a wrong-key regression would fail here.
#[test]
fn an_in_flight_units_node_status_is_the_shared_blocker_classification() {
    use rigger::blocker;

    // A unit mid-review (verified): the shared classifier tags it `reviewing`.
    let events = positioned(vec![
        ev("UnitStarted", r#"{"id":"u30-c1"}"#),
        ev("UnitStatus", r#"{"id":"u30-c1","status":"verified"}"#),
    ]);

    // Independently compute what the SHARED classifier says for this unit.
    let blockers = blocker::from_events(&events, 3).unwrap();
    let kind = blockers
        .iter()
        .find(|b| b.subject() == "u30-c1")
        .expect("the classifier tags the in-flight unit")
        .kind_tag();
    assert_eq!(
        kind, "reviewing",
        "precondition: the shared classifier tags a verified unit reviewing"
    );

    let v = state_json(&events, &[], &HashMap::new());
    let unit = child(&v["tree"][0], "u30-c1");
    assert_eq!(
        unit["status"], kind,
        "an in-flight unit's node status IS the shared blocker kind_tag, reused not re-derived"
    );
}

// ---------------------------------------------------------------------------
// 9. Sentinel: a non-terminal unit with NO blocker entry falls back to its ledger status.
// ---------------------------------------------------------------------------

/// The `unit_live_status` sentinel - a non-terminal, non-escalated unit that carries no blocker
/// entry falls back to its ledger status - is unreachable through `build_state` (the shared
/// classifier tags every non-integrated unit), so it is driven here directly through the public
/// `build_run_tree` with an EMPTY blocker slice: a fresh (grounding) unit with no classification
/// must render its own ledger status, never an empty or panicking node.
#[test]
fn a_unit_with_no_blocker_entry_falls_back_to_its_ledger_status() {
    use rigger::ledger;

    let events = positioned(vec![ev("UnitStarted", r#"{"id":"u30-c1"}"#)]);
    let run = ledger::project(&events).expect("the run projects");
    // What the ledger itself says this non-terminal unit's status is - the sentinel's fallback.
    let ledger_status = run.units["u30-c1"].status.as_str();
    assert_eq!(
        ledger_status, "grounding",
        "precondition: a freshly started unit is grounding"
    );

    // The no-blocker case: an empty classification slice, so the sentinel is the only path.
    let tree = dash::build_run_tree(&events, &run, &[], &[]).expect("the tree projects");

    let spec = &tree[0];
    let unit = spec
        .children
        .iter()
        .find(|n| n.label == "u30-c1")
        .expect("the unit is in the tree");
    assert_eq!(
        unit.status, ledger_status,
        "a non-terminal unit with no blocker entry falls back to its ledger status via the sentinel"
    );
}

// ---------------------------------------------------------------------------
// 10. Spec-root FAILED arm: a lingering Failed unit (no blocker entry) surfaces at the spec root.
// ---------------------------------------------------------------------------

/// The spec-root rollup's `failed` arm (distinct from the `escalated` arm and the ROLE-level
/// rollup): a unit whose live status is `failed` surfaces its failure at the spec root instead of
/// rendering `building` forever. Through the real `/api/state` boundary the shared classifier tags
/// every non-integrated unit (so a `Failed` unit reads `reject-recurrence`, never the bare
/// `failed`), leaving this arm reachable only via the direct public `build_run_tree` with an EMPTY
/// blocker slice - the same no-blocker path the sentinel uses. Pinned here so dropping the arm
/// (which regresses the spec root to `building`) fails loudly.
#[test]
fn a_failed_unit_with_no_blocker_entry_surfaces_at_the_spec_root() {
    use rigger::ledger;

    let events = positioned(vec![
        ev("UnitStarted", r#"{"id":"u30-c1"}"#),
        ev("UnitFailed", r#"{"id":"u30-c1","attempts":1}"#),
    ]);
    let run = ledger::project(&events).expect("the run projects");
    assert_eq!(
        run.units["u30-c1"].status.as_str(),
        "failed",
        "precondition: the unit's ledger status is failed"
    );

    // Empty blocker slice, so `unit_live_status` falls back to the ledger status `failed` (the
    // classifier's `reject-recurrence` never intervenes) and the spec-root failed arm is the path.
    let tree = dash::build_run_tree(&events, &run, &[], &[]).expect("the tree projects");
    let spec = &tree[0];
    let unit = spec
        .children
        .iter()
        .find(|n| n.label == "u30-c1")
        .expect("the unit is in the tree");
    assert_eq!(
        unit.status, "failed",
        "the unit node carries its failed live status via the no-blocker sentinel"
    );
    assert_eq!(
        spec.status, "failed",
        "a failed child surfaces at the spec root, not a perpetual building"
    );
}
