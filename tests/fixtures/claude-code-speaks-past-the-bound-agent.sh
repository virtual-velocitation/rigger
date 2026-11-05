#!/bin/sh
# A fixture "agent" for LIVENESS (spec 105): a session that speaks a line each second for 5 s
# and ends in a `result`. Its age passes a 3 s bound while the time since its last line never
# does, so a host with no beat source, which counts from the last line, reads it through.
# Reads and discards the host's first stdin message like every sibling fixture.
self=$(readlink "$0") || self=$0
. "$(dirname "$self")/launch-args.sh"
sid=$(launched_session bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb "$@")
IFS= read -r line
echo '{"type":"system","subtype":"init","session_id":"'"$sid"'","model":"claude-haiku-4-5","mcp_servers":[{"name":"rigger","status":"connected"}]}'
spoken=0
while [ "$spoken" -lt 5 ]; do
  sleep 1
  echo '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Working on it."}]},"session_id":"bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"}'
  spoken=$((spoken + 1))
done
echo '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"done: spoke past the bound","session_id":"bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb","total_cost_usd":0,"usage":{"input_tokens":1,"output_tokens":1},"permission_denials":[]}'
