#!/bin/sh
# A fixture "agent" for LIVENESS (spec 105): a session that announces two retries, each with
# its own delay and category, then ends in a `result`. Proves the host appends one `ApiRetry`
# per `api_retry` line in stream order: two lines with their own values tell one record per
# line from one record per session.
# Reads and discards the host's first stdin message like every sibling fixture.
IFS= read -r line
echo '{"type":"system","subtype":"init","session_id":"66666666-6666-4666-8666-666666666666","model":"claude-haiku-4-5","mcp_servers":[]}'
echo '{"type":"system","subtype":"api_retry","attempt":1,"max_retries":10,"retry_delay_ms":618,"error_status":429,"error":"rate_limit"}'
echo '{"type":"system","subtype":"api_retry","attempt":2,"max_retries":10,"retry_delay_ms":30000,"error_status":529,"error":"overloaded"}'
echo '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"done after two retries","session_id":"66666666-6666-4666-8666-666666666666","total_cost_usd":0.01,"usage":{"input_tokens":10,"output_tokens":5},"permission_denials":[]}'
