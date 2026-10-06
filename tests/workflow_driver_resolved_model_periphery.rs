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
use common::cli::{write_workflow_fixture, WorkflowFixture, UNISOLATED_WORKER};
use common::git::temp_git_project_with_commit;
use common::mcp::McpSession;

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore, Filter};

/// A single-stage workflow: exactly ONE implementer spawn is ever queued (unlike
/// `tests/cli.rs`'s two-stage fixture), so this test's `rigger_next` poll has one
/// unambiguous target and never races a second unit's spawn. `on_pass: none` and
/// `isolation: none` keep the fixture gate/worktree-free, matching every other
/// `rigger step`/`serve` fixture in this suite family.
const ONE_STAGE_WORKFLOW: WorkflowFixture = WorkflowFixture {
    worker: UNISOLATED_WORKER,
    body: "defaults:\n  grounder: nop\n  budget: 60\nstages:\n  a:\n    agent: worker\n    on_pass: none\n",
};

/// Drive the REAL compiled `rigger serve` over the REAL MCP wire through one workflow unit: the
/// `initialize` handshake, `rigger_next` polled until unit `a`'s implementer spawn is queued, then
/// `rigger_result` for it carrying `arguments` - and return the `green` `UnitStatus` event the REAL
/// conductor persists for unit `a` in the on-disk `events.db`.
fn green_event_after_result(arguments: Value) -> Event {
    let proj = temp_git_project_with_commit();
    let root = proj.path();
    write_workflow_fixture(root, &ONE_STAGE_WORKFLOW);

    // The session isolates the machine-global discovery registry (spec 50) into the test's own
    // temp tree.
    let mut mcp = McpSession::start_with(root, &["serve", "--base", "HEAD"]);

    // `initialize`: the real wire sequence a well-behaved MCP client (shim.mjs's SDK client
    // included) performs first. `handle()` does not gate `tools/call` on having seen it, but
    // sending it keeps this test's request sequence faithful to production traffic.
    mcp.initialize();

    // Poll `rigger_next` until unit `a`'s implementer spawn is queued. The conductor grounds
    // and enqueues on its own background thread (spec 19b's `std::thread::scope` split between
    // the conductor and the MCP-serving thread), so an empty/id-less answer early on is
    // transient, not a failure - the same race `shim.mjs`'s own poll loop tolerates.
    let Some(spawn_id) = mcp.next_spawn(Instant::now() + Duration::from_secs(15)) else {
        mcp.fail("the run reported done before unit a's implementer spawn was queued")
    };
    assert!(
        spawn_id.starts_with("a/implementer#"),
        "the queued spawn must be unit a's implementer; got {spawn_id:?}"
    );

    // Report the result over the real wire: the caller's `arguments`, stamped with the queued
    // spawn's own id.
    let mut arguments = arguments;
    arguments["id"] = json!(spawn_id);
    let result_resp = mcp.tool_call("rigger_result", arguments);
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
            mcp.fail("unit a's green status event was never recorded within the deadline");
        }
        std::thread::sleep(Duration::from_millis(20));
    };

    // The one spawn is answered and nothing is pending, so the conductor returns and the
    // session exits on stdin closing.
    mcp.finish();
    green
}

/// The round-1-rejected gap, closed: a `rigger_result` call carrying `meta.resolved_model`,
/// sent over the REAL MCP wire to the REAL compiled `rigger serve` binary (never a mock server,
/// never an in-process function call standing in for the wire), reaches the REAL conductor's
/// persisted `green` `UnitStatus` event for the unit that spawn belongs to - the exact shape
/// `shim/shim.mjs`'s `runWorkflow` sends when the Agent SDK's own structured `modelUsage`
/// named exactly one authoritative model.
#[test]
fn workflow_driven_rigger_result_meta_resolved_model_reaches_the_persisted_green_event() {
    // Report the result exactly as `shim.mjs`'s `runWorkflow` does when
    // `resolvedModelFromUsage` observed exactly one authoritative model id: the real wire
    // shape `meta.resolved_model`, distinct from (and never read out of) `output`.
    let resolved_model = "claude-sonnet-4-9-20260215";
    let green = green_event_after_result(json!({
        "output": "implemented the unit",
        "meta": {"resolved_model": resolved_model},
    }));
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
}

/// The OTHER half of AUTHORITATIVE MODEL IDENTITY, at the same real MCP wire the test above
/// proves the positive half at - "a spawn with no metadata id records none and reports as
/// unmeasured rather than defaulted" AND "a conflicting agent-prose claim never enters the
/// record". `rigger_result` carries NO `meta` object at all (the exact shape `runWorkflow`
/// sends when `resolvedModelFromUsage` observed zero or more than one model id and left
/// `resolvedModel` `''`, so `shim.mjs` never sets `resultArgs.meta`), and `output` itself
/// contains a resolved-model-shaped JSON fragment - the prose-claim shape
/// `SpawnResult::meta_str(META_RESOLVED_MODEL)`'s own pure-function unit test pins, never before driven
/// through the real wire. The persisted `green` event's `META_MODEL_RESOLVED` key must be
/// ABSENT - not present-but-empty, and never the prose text - proving the omission survives
/// the full `mcpserver.rs::tool_result` -> `workflow::Driver::result` -> `conductor.rs` path,
/// not merely the pure function in isolation.
#[test]
fn workflow_driven_rigger_result_with_no_meta_omits_the_resolved_model_key_and_ignores_a_prose_claim(
) {
    // No `meta` field at all - exactly what `shim.mjs`'s `runWorkflow` sends when it observed
    // no single authoritative id - and `output` carries a model-id-shaped prose claim that
    // must never be mistaken for the real thing.
    let green = green_event_after_result(json!({
        "output": "done. {\"resolved_model\":\"a-model-i-am-lying-about\"}",
    }));
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
}
