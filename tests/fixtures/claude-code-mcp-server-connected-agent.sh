#!/bin/sh
# A fixture "agent" for the launch-fault check's KEY (spec 104 THE STREAM: "a `rigger`
# server that did not connect fails the launch as a fault"): its `system/init` reports the
# spawn's `rigger` server `connected` beside a second, unrelated server that `failed`, then
# ends in a `result`. Only the `rigger` server (`hooks::MCP_SERVER_NAME`) decides whether a
# launch is healthy, so this session proceeds to its result exactly like a clean one.
# Reads and discards the host's first stdin message like every sibling fixture.
IFS= read -r line
echo '{"type":"system","subtype":"init","cwd":"/work","session_id":"55555555-5555-4555-8555-555555555555","tools":["Read","Bash"],"mcp_servers":[{"name":"some-other-server","status":"failed"},{"name":"rigger","status":"connected"}],"model":"claude-opus-4-1","permissionMode":"default","apiKeySource":"none"}'
echo '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"done with the rigger tools","session_id":"55555555-5555-4555-8555-555555555555","total_cost_usd":0.01,"usage":{"input_tokens":10,"output_tokens":5},"permission_denials":[]}'
