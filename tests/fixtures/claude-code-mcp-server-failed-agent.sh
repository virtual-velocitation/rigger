#!/bin/sh
# A fixture "agent" for a LAUNCH FAULT (spec 104 THE STREAM: "a `rigger` server that did not
# connect fails the launch as a fault"): replays the `system/init` a real claude 2.1.283
# emitted on 2026-09-28 when the spawn's `rigger mcp --spawn <id>` server exited before it
# connected - `mcp_servers: [{"name":"rigger","status":"failed"}]` - and then, like the real
# session did, carries on with its task (here: a sleep standing in for the toolless work,
# then a successful-looking `result`). A host that reads that init as a success lets the
# task run to that result; the host this pins stops the session at the init line.
#
# Records its own pid FIRST (so the test can prove the session did not outlive the fault,
# same rationale as claude-code-invalid-utf8-agent.sh), then reads and discards the host's
# first stream-json message like every sibling fixture.
#
# The stand-in work starts BEFORE the init line and the session waits on it after. A `sleep`
# forked after that line races the host's descendant snapshot, taken the moment the host reads
# the line: forked after the snapshot but before this shell is ended, it escapes the stop
# holding stdout and stderr, and the host's pipe joins wait out THE STOP's grace (most runs
# pinned to one CPU, now and then on a loaded CI runner). That escaped process is THE STOP's
# bounded-join case, pinned by
# spawn_stop_returns_within_bound_when_a_descendant_has_already_escaped_the_childs_tree; this
# fixture pins the stop at init, so its work is in the tree before the host can read the line.
self=$(readlink "$0") || self=$0
. "$(dirname "$self")/launch-args.sh"
sid=$(launched_session 44444444-4444-4444-8444-444444444444 "$@")
if [ -n "$RIGGER_TEST_MCP_FAILED_PID_FILE" ]; then
    echo "$$" >"$RIGGER_TEST_MCP_FAILED_PID_FILE"
fi
IFS= read -r line
sleep 30 &
work=$!
echo '{"type":"system","subtype":"init","cwd":"/work","session_id":"'"$sid"'","tools":["Read","Bash"],"mcp_servers":[{"name":"rigger","status":"failed"}],"model":"claude-opus-4-1","permissionMode":"default","apiKeySource":"none"}'
wait "$work"
echo '{"type":"result","subtype":"success","is_error":false,"num_turns":4,"result":"done without any rigger tools","session_id":"44444444-4444-4444-8444-444444444444","total_cost_usd":0.01,"usage":{"input_tokens":10,"output_tokens":5},"permission_denials":[]}'
