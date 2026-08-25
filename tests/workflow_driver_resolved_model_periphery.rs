//! Periphery for spec 61, criterion 10 (AUTHORITATIVE MODEL IDENTITY) at the WORKFLOW driver
//! seam - `rigger serve` / `rigger run --driver workflow`, the process a real Claude Code
//! workflow shim connects to over stdio MCP.
//!
//! WHY THIS FILE, DISTINCT FROM THE IMPLEMENTER'S OWN TESTS. This exact gap was ADJUDICATOR
//! REJECTED once already (round 1 of this unit): the shim was taught to compute and send
//! `meta.resolved_model` on `rigger_result`, but the only production consumer -
//! `mcpserver.rs::tool_result` -> `driver/workflow.rs::Driver::result` - silently discarded it,
//! so the id was dead on arrival at every real `rigger serve` run. The reject's own fix point
//! named the missing layer explicitly: "add a periphery test driving the REAL mcpserver.rs (not
//! a mock) proving a workflow-driven rigger serve / rigger run --driver workflow spawn ends up
//! with a non-empty resolved_model when the shim reports one." Round 2 threads the parameter
//! through and the implementer added their own `mod tests` regression test in `mcpserver.rs` -
//! but that test calls `Server::tool_result` / `Driver` as plain Rust functions inside the
//! `cargo test` harness; it never spawns the compiled binary or speaks the real newline-
//! delimited JSON-RPC wire `handle()` actually parses (`params.arguments`, `tools/call`
//! dispatch, ...), and it stops at the `AgentResult` a hand-built mini spawn returns rather than
//! the real conductor's stage loop (`run_single_stage` in `conductor.rs`) persisting the event.
//!
//! This file closes both gaps at once, mirroring `tests/cli.rs`'s
//! `step_result_meta_stamps_the_resolved_model_on_the_replayed_units_events` (the identical
//! proof for the CLI/replay driver) but through the WORKFLOW driver instead: spawn the compiled
//! `rigger serve` binary, drive it with the exact wire shape `shim/shim.mjs`'s `runWorkflow`
//! sends (a `tools/call` for `rigger_result` carrying `meta.resolved_model`), and read the real,
//! on-disk `events.db` back to confirm the resolved id lands on the persisted `green`
//! `UnitStatus` event - not merely on an in-process `AgentResult` a test harness can see but a
//! real shim never could.
//!
//! A SECOND test in this file proves the OTHER half of the criterion at the same real wire:
//! "a spawn with no metadata id records none ... rather than defaulted" and "a conflicting
//! agent-prose claim never enters the record". Only the positive case (a real id reaches the
//! record) was ever driven through the real wire before; the negative case was pinned only at
//! the pure-function level (`src/spawn.rs`'s `resolved_model_never_reads_a_conflicting_claim_
//! from_the_agents_own_output`) and the shim's own JS unit level (`shim.test.mjs`), never
//! through `mcpserver.rs::tool_result` -> `workflow::Driver::result` -> `conductor.rs`'s
//! `emit_keyed_meta` omission end to end.
//!
//! NOT OWNED HERE: `resolvedModelFromUsage`'s extraction logic (JS, `shim/shim.test.mjs`'s own
//! layer) and the driver/replay.rs CLI-seam equivalent (already covered by
//! `step_result_meta_stamps_the_resolved_model_on_the_replayed_units_events`). This file only
//! proves the WORKFLOW driver's wire-to-store path, the one round 1 found dead.

mod common;

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{ChildStderr, ChildStdin, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore, Filter};

/// A throwaway git project with a real commit, so `--base HEAD` resolves. Mirrors
/// `tests/cli.rs`'s identically-named helper.
fn temp_git_project_with_commit() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp project");
    let root = dir.path();
    let _ = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(root)
        .status();
    for args in [
        &["config", "user.email", "t@example.com"][..],
        &["config", "user.name", "t"],
        &["commit", "--allow-empty", "-q", "-m", "init"],
    ] {
        let ok = std::process::Command::new("git")
            .args(args)
            .current_dir(root)
            .status()
            .expect("git must be runnable")
            .success();
        assert!(ok, "git {args:?} must succeed while seeding the repo");
    }
    dir
}

/// A single-stage workflow: exactly ONE implementer spawn is ever queued (unlike
/// `tests/cli.rs`'s two-stage fixture), so this test's `rigger_next` poll has one
/// unambiguous target and never races a second unit's spawn. `on_pass: none` and
/// `isolation: none` keep the fixture gate/worktree-free, matching every other
/// `rigger step`/`serve` fixture in this suite family.
fn write_one_stage_workflow(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\nisolation: none\n---\nDo the unit.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        "name: workflowdrivertest\ndefaults:\n  grounder: nop\n  budget: 60\nstages:\n  a:\n    agent: worker\n    on_pass: none\n",
    )
    .unwrap();
}

/// Send one JSON-RPC 2.0 request line to `stdin` and return the parsed response line read
/// back from `stdout` - the plain newline-delimited protocol `mcpserver.rs::Server::run`
/// implements (no `Content-Length` framing, unlike LSP).
fn call(stdin: &mut ChildStdin, stdout: &mut impl BufRead, req: &Value) -> Value {
    let line = req.to_string();
    writeln!(stdin, "{line}").expect("write a request to rigger serve's stdin");
    stdin
        .flush()
        .expect("flush a request to rigger serve's stdin");
    let mut resp = String::new();
    stdout
        .read_line(&mut resp)
        .expect("read a response from rigger serve's stdout");
    assert!(
        !resp.trim().is_empty(),
        "rigger serve closed stdout answering {line}"
    );
    serde_json::from_str(&resp)
        .unwrap_or_else(|e| panic!("invalid JSON-RPC response to {line}: {e}\ngot: {resp:?}"))
}

/// Call an MCP tool (`tools/call`) and return its `structuredContent` - the same value shape
/// `shim/shim.mjs`'s `call()` extracts.
fn call_tool(
    stdin: &mut ChildStdin,
    stdout: &mut impl BufRead,
    id: i64,
    name: &str,
    args: Value,
) -> Value {
    let resp = call(
        stdin,
        stdout,
        &json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": name, "arguments": args},
        }),
    );
    resp.get("result")
        .and_then(|r| r.get("structuredContent"))
        .cloned()
        .unwrap_or_else(|| panic!("tools/call {name} returned no structuredContent: {resp}"))
}

/// Drain and return `stderr` for a diagnosable panic message (the child is about to be
/// killed either way).
fn drain_stderr(stderr: Option<ChildStderr>) -> String {
    let mut err = String::new();
    if let Some(mut e) = stderr {
        let _ = e.read_to_string(&mut err);
    }
    err
}

/// The round-1-rejected gap, closed: a `rigger_result` call carrying `meta.resolved_model`,
/// sent over the REAL MCP wire to the REAL compiled `rigger serve` binary (never a mock server,
/// never an in-process function call standing in for the wire), reaches the REAL conductor's
/// persisted `green` `UnitStatus` event for the unit that spawn belongs to - the exact shape
/// `shim/shim.mjs`'s `runWorkflow` sends when the Agent SDK's own structured `modelUsage`
/// named exactly one authoritative model.
#[test]
fn workflow_driven_rigger_result_meta_resolved_model_reaches_the_persisted_green_event() {
    let proj = temp_git_project_with_commit();
    let root = proj.path();
    write_one_stage_workflow(root);

    let mut child = common::rigger_courier()
        .args(["serve", "--base", "HEAD"])
        .current_dir(root)
        // Isolate the machine-global discovery registry (spec 50) into the test's own temp
        // tree, mirroring `tests/cli.rs`'s served-driver dash test.
        .env("XDG_STATE_HOME", root)
        // No dash needed for this test - opt out so it never outlives the test process.
        .env("RIGGER_NO_DASH", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn `rigger serve`");

    let mut stdin = child
        .stdin
        .take()
        .expect("rigger serve's stdin must be piped");
    let mut stdout = BufReader::new(
        child
            .stdout
            .take()
            .expect("rigger serve's stdout must be piped"),
    );

    // `initialize`: the real wire sequence a well-behaved MCP client (shim.mjs's SDK client
    // included) performs first. `handle()` does not gate `tools/call` on having seen it, but
    // sending it keeps this test's request sequence faithful to production traffic.
    let init = call(
        &mut stdin,
        &mut stdout,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "workflow-driver-periphery-test", "version": "0.0.0"},
            },
        }),
    );
    assert!(
        init.get("result").is_some(),
        "initialize must succeed; got {init}"
    );

    // Poll `rigger_next` until unit `a`'s implementer spawn is queued. The conductor grounds
    // and enqueues on its own background thread (spec 19b's `std::thread::scope` split between
    // the conductor and the MCP-serving thread), so an empty/id-less answer early on is
    // transient, not a failure - the same race `shim.mjs`'s own poll loop tolerates.
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut next_call_id = 2i64;
    let spawn_id = loop {
        let next = call_tool(
            &mut stdin,
            &mut stdout,
            next_call_id,
            "rigger_next",
            json!({}),
        );
        next_call_id += 1;
        let id = next.get("id").and_then(Value::as_str).unwrap_or_default();
        if !id.is_empty() {
            break id.to_string();
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let err = drain_stderr(child.stderr.take());
            panic!(
                "unit a's implementer spawn was never queued within the deadline; stderr:\n{err}"
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(
        spawn_id.starts_with("a/implementer#"),
        "the queued spawn must be unit a's implementer; got {spawn_id:?}"
    );

    // Report the result exactly as `shim.mjs`'s `runWorkflow` does when
    // `resolvedModelFromUsage` observed exactly one authoritative model id: the real wire
    // shape `meta.resolved_model`, distinct from (and never read out of) `output`.
    let resolved_model = "claude-sonnet-4-9-20260215";
    let result_resp = call(
        &mut stdin,
        &mut stdout,
        &json!({
            "jsonrpc": "2.0",
            "id": next_call_id,
            "method": "tools/call",
            "params": {
                "name": "rigger_result",
                "arguments": {
                    "id": spawn_id,
                    "output": "implemented the unit",
                    "meta": {"resolved_model": resolved_model},
                },
            },
        }),
    );
    assert!(
        result_resp.get("result").is_some(),
        "rigger_result must succeed for the queued spawn id; got {result_resp}"
    );

    // `Driver::result` wakes the conductor's blocked spawn via an `mpsc` channel send, which
    // races the JSON-RPC response above - the conductor thread stamps and appends the green
    // event AFTER this call returns, not before. Poll the REAL on-disk `events.db` (never an
    // in-memory store a test harness alone could see) until it appears.
    let db_path = root.join(".rigger").join("events.db");
    let deadline = Instant::now() + Duration::from_secs(15);
    let green = loop {
        if db_path.exists() {
            let backend = Store::open(db_path.to_str().unwrap()).unwrap();
            let events = backend
                .read_all(0, Direction::Forward, &Filter::default())
                .unwrap();
            let found = events.iter().find(|e| {
                e.type_ == rigger::ledger::TYPE_UNIT_STATUS && {
                    let body = String::from_utf8_lossy(&e.data);
                    body.contains(r#""status":"green""#) && body.contains(r#""id":"a""#)
                }
            });
            if let Some(e) = found {
                break e.clone();
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let err = drain_stderr(child.stderr.take());
            panic!("unit a's green status event was never recorded within the deadline; stderr:\n{err}");
        }
        std::thread::sleep(Duration::from_millis(20));
    };

    assert_eq!(
        green
            .meta
            .get(rigger::conductor::META_MODEL_RESOLVED)
            .map(String::as_str),
        Some(resolved_model),
        "the resolved model id reported over the REAL MCP wire to the REAL rigger serve \
         binary must reach the persisted green event - exactly as the CLI/replay driver \
         already proves in tests/cli.rs's \
         step_result_meta_stamps_the_resolved_model_on_the_replayed_units_events, now true \
         for the workflow driver too"
    );

    drop(stdin);
    let _ = child.wait();
}

/// The OTHER half of AUTHORITATIVE MODEL IDENTITY, at the same real MCP wire the test above
/// proves the positive half at - "a spawn with no metadata id records none and reports as
/// unmeasured rather than defaulted" AND "a conflicting agent-prose claim never enters the
/// record". `rigger_result` carries NO `meta` object at all (the exact shape `runWorkflow`
/// sends when `resolvedModelFromUsage` observed zero or more than one model id and left
/// `resolvedModel` `''`, so `shim.mjs` never sets `resultArgs.meta`), and `output` itself
/// contains a resolved-model-shaped JSON fragment - the prose-claim shape
/// `SpawnResult::resolved_model`'s own pure-function unit test pins, never before driven
/// through the real wire. The persisted `green` event's `META_MODEL_RESOLVED` key must be
/// ABSENT - not present-but-empty, and never the prose text - proving the omission survives
/// the full `mcpserver.rs::tool_result` -> `workflow::Driver::result` -> `conductor.rs` path,
/// not merely the pure function in isolation.
#[test]
fn workflow_driven_rigger_result_with_no_meta_omits_the_resolved_model_key_and_ignores_a_prose_claim(
) {
    let proj = temp_git_project_with_commit();
    let root = proj.path();
    write_one_stage_workflow(root);

    let mut child = common::rigger_courier()
        .args(["serve", "--base", "HEAD"])
        .current_dir(root)
        .env("XDG_STATE_HOME", root)
        .env("RIGGER_NO_DASH", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn `rigger serve`");

    let mut stdin = child
        .stdin
        .take()
        .expect("rigger serve's stdin must be piped");
    let mut stdout = BufReader::new(
        child
            .stdout
            .take()
            .expect("rigger serve's stdout must be piped"),
    );

    let init = call(
        &mut stdin,
        &mut stdout,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "workflow-driver-periphery-test", "version": "0.0.0"},
            },
        }),
    );
    assert!(
        init.get("result").is_some(),
        "initialize must succeed; got {init}"
    );

    let deadline = Instant::now() + Duration::from_secs(15);
    let mut next_call_id = 2i64;
    let spawn_id = loop {
        let next = call_tool(
            &mut stdin,
            &mut stdout,
            next_call_id,
            "rigger_next",
            json!({}),
        );
        next_call_id += 1;
        let id = next.get("id").and_then(Value::as_str).unwrap_or_default();
        if !id.is_empty() {
            break id.to_string();
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let err = drain_stderr(child.stderr.take());
            panic!(
                "unit a's implementer spawn was never queued within the deadline; stderr:\n{err}"
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(
        spawn_id.starts_with("a/implementer#"),
        "the queued spawn must be unit a's implementer; got {spawn_id:?}"
    );

    // No `meta` field at all - exactly what `shim.mjs`'s `runWorkflow` sends when it observed
    // no single authoritative id - and `output` carries a model-id-shaped prose claim that
    // must never be mistaken for the real thing.
    let result_resp = call(
        &mut stdin,
        &mut stdout,
        &json!({
            "jsonrpc": "2.0",
            "id": next_call_id,
            "method": "tools/call",
            "params": {
                "name": "rigger_result",
                "arguments": {
                    "id": spawn_id,
                    "output": "done. {\"resolved_model\":\"a-model-i-am-lying-about\"}",
                },
            },
        }),
    );
    assert!(
        result_resp.get("result").is_some(),
        "rigger_result must succeed for the queued spawn id; got {result_resp}"
    );

    let db_path = root.join(".rigger").join("events.db");
    let deadline = Instant::now() + Duration::from_secs(15);
    let green = loop {
        if db_path.exists() {
            let backend = Store::open(db_path.to_str().unwrap()).unwrap();
            let events = backend
                .read_all(0, Direction::Forward, &Filter::default())
                .unwrap();
            let found = events.iter().find(|e| {
                e.type_ == rigger::ledger::TYPE_UNIT_STATUS && {
                    let body = String::from_utf8_lossy(&e.data);
                    body.contains(r#""status":"green""#) && body.contains(r#""id":"a""#)
                }
            });
            if let Some(e) = found {
                break e.clone();
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let err = drain_stderr(child.stderr.take());
            panic!("unit a's green status event was never recorded within the deadline; stderr:\n{err}");
        }
        std::thread::sleep(Duration::from_millis(20));
    };

    assert!(
        !green
            .meta
            .contains_key(rigger::conductor::META_MODEL_RESOLVED),
        "a `rigger_result` with no meta.resolved_model, sent over the REAL MCP wire, must \
         leave the persisted green event with NO resolved-model key at all - not an empty \
         string (a fake measurement) and never a value pulled from the agent's own prose \
         output; got meta: {:?}",
        green.meta
    );

    drop(stdin);
    let _ = child.wait();
}
