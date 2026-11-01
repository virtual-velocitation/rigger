#!/bin/sh
# A fixture "agent" for LIVENESS (spec 105): a session that announces a retry, then stays
# silent until the file `$RIGGER_TEST_RETRY_RELEASE_FILE` names exists, then ends in a `result`.
# The silence is the delay the record exists to explain, so a test reads the record while this
# session is still inside it. The bound only keeps a test that panicked from waiting forever.
# Reads and discards the host's first stdin message like every sibling fixture.
IFS= read -r line
echo '{"type":"system","subtype":"init","session_id":"88888888-8888-4888-8888-888888888888","model":"claude-haiku-4-5","mcp_servers":[]}'
echo '{"type":"system","subtype":"api_retry","attempt":1,"max_retries":10,"retry_delay_ms":30000,"error_status":529,"error":"overloaded"}'
waited=0
while [ ! -e "$RIGGER_TEST_RETRY_RELEASE_FILE" ] && [ "$waited" -lt 600 ]; do
  sleep 0.1
  waited=$((waited + 1))
done
echo '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"done after the wait","session_id":"88888888-8888-4888-8888-888888888888","total_cost_usd":0.01,"usage":{"input_tokens":10,"output_tokens":5},"permission_denials":[]}'
