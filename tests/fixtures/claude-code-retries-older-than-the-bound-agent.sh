#!/bin/sh
# A fixture "agent" for LIVENESS (spec 105): a session that speaks a line each second for 4 s,
# older than a 3 s bound by the time it announces a 30 s retry, stays silent 4 s inside that
# delay and ends in a `result`. The silent seconds count from its last line, so every one of
# them is excused; counted from its launch, the 4 s it spoke would pass the bound.
# Reads and discards the host's first stdin message like every sibling fixture.
IFS= read -r line
echo '{"type":"system","subtype":"init","session_id":"cccccccc-cccc-4ccc-8ccc-cccccccccccc","model":"claude-haiku-4-5","mcp_servers":[{"name":"rigger","status":"connected"}]}'
spoken=0
while [ "$spoken" -lt 4 ]; do
  sleep 1
  echo '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Working on it."}]},"session_id":"cccccccc-cccc-4ccc-8ccc-cccccccccccc"}'
  spoken=$((spoken + 1))
done
echo '{"type":"system","subtype":"api_retry","attempt":1,"max_retries":10,"retry_delay_ms":30000,"error_status":529,"error":"overloaded"}'
sleep 4
echo '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"done: waited the delay out at its age","session_id":"cccccccc-cccc-4ccc-8ccc-cccccccccccc","total_cost_usd":0,"usage":{"input_tokens":1,"output_tokens":1},"permission_denials":[]}'
