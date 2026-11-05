#!/bin/sh
# A fixture "agent" for LIVENESS (spec 105): a session that announces a 2 s retry and stays
# silent 20 s, past the delay and past the host's unbounded wake-up interval (`UNBOUNDED_POLL`,
# 5 s), then ends in a `result`. Its first wake excuses every second, so only a later wake can
# stop it, and the result is what a host that never wakes again would read.
# Reads and discards the host's first stdin message like every sibling fixture.
self=$(readlink "$0") || self=$0
. "$(dirname "$self")/launch-args.sh"
sid=$(launched_session dddddddd-dddd-4ddd-8ddd-dddddddddddd "$@")
IFS= read -r line
echo '{"type":"system","subtype":"init","session_id":"'"$sid"'","model":"claude-haiku-4-5","mcp_servers":[{"name":"rigger","status":"connected"}]}'
echo '{"type":"system","subtype":"api_retry","attempt":1,"max_retries":10,"retry_delay_ms":2000,"error_status":529,"error":"overloaded"}'
sleep 20
echo '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"done: hung past its delay","session_id":"dddddddd-dddd-4ddd-8ddd-dddddddddddd","total_cost_usd":0,"usage":{"input_tokens":1,"output_tokens":1},"permission_denials":[]}'
