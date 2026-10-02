#!/bin/sh
# A fixture "agent" for THE STREAM's denial count (spec 104: `meta.permission_denials`):
# replays the shape a real claude 2.1.283 emitted on 2026-09-28 when a PreToolUse hook (the
# graph-first lookup guard) denied a text search. A hook block emits NO
# `system/permission_denied` stream event, yet the final `result` lists it in its own
# `permission_denials` array - so a host counting stream events records 0 where the session
# itself reports 1. Reads and discards the host's first stdin message like every sibling
# fixture.
IFS= read -r line
echo '{"type":"system","subtype":"init","cwd":"/work","session_id":"66666666-6666-4666-8666-666666666666","tools":["Read","Bash"],"mcp_servers":[{"name":"rigger","status":"connected"}],"model":"claude-opus-4-1","permissionMode":"default","apiKeySource":"none"}'
echo '{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_01HookBlockedSearch","name":"Bash","input":{"command":"grep -rn needle src"}}]},"session_id":"66666666-6666-4666-8666-666666666666"}'
echo '{"type":"result","subtype":"success","is_error":false,"num_turns":2,"result":"looked it up in the graph instead","session_id":"66666666-6666-4666-8666-666666666666","total_cost_usd":0.02,"usage":{"input_tokens":20,"output_tokens":8},"permission_denials":[{"tool_name":"Bash","tool_use_id":"toolu_01HookBlockedSearch","tool_input":{"command":"grep -rn needle src"}}]}'
