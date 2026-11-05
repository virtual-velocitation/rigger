#!/bin/sh
# A fixture "agent" for LIVENESS (spec 105): a session whose `api_retry` lines stray from the
# shape a current host writes, then end WITHOUT a `result`. A category no binary knows, a
# negative delay and a line naming neither field each still leave one `ApiRetry`, and the line
# naming no category leaves the session's class on the last category a line did name.
# Reads and discards the host's first stdin message like every sibling fixture.
self=$(readlink "$0") || self=$0
. "$(dirname "$self")/launch-args.sh"
sid=$(launched_session 77777777-7777-4777-8777-777777777777 "$@")
IFS= read -r line
echo '{"type":"system","subtype":"init","session_id":"'"$sid"'","model":"claude-haiku-4-5","mcp_servers":[]}'
echo '{"type":"system","subtype":"api_retry","attempt":1,"max_retries":10,"retry_delay_ms":1500,"error_status":500,"error":"a_category_no_binary_knows"}'
echo '{"type":"system","subtype":"api_retry","attempt":2,"max_retries":10,"retry_delay_ms":-618,"error_status":529,"error":"overloaded"}'
echo '{"type":"system","subtype":"api_retry","attempt":3,"max_retries":10,"retry_delay_ms":618,"error_status":429,"error":"rate_limit"}'
echo '{"type":"system","subtype":"api_retry","attempt":4,"max_retries":10}'
