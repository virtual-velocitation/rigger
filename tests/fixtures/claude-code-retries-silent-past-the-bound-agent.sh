#!/bin/sh
# A fixture "agent" for LIVENESS (spec 105): a session that announces a 30 s retry, stays silent
# for 4 s, which is past the low `max_wall_clock` a test bounds it to and inside the delay it
# announced, then ends in a `result`. A host reading the announced delay lets it finish; a host
# counting from the last line stops it.
# Reads and discards the host's first stdin message like every sibling fixture.
IFS= read -r line
echo '{"type":"system","subtype":"init","session_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","model":"claude-haiku-4-5","mcp_servers":[{"name":"rigger","status":"connected"}]}'
echo '{"type":"system","subtype":"api_retry","attempt":1,"max_retries":10,"retry_delay_ms":30000,"error_status":529,"error":"overloaded"}'
sleep 4
echo '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"done: waited the delay out","session_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","total_cost_usd":0,"usage":{"input_tokens":1,"output_tokens":1},"permission_denials":[]}'
