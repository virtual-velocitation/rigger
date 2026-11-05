#!/bin/sh
# A fixture "agent" for THE STREAM (spec 104 criterion 2): a session that ends WITHOUT
# ever emitting a `result` message - the CONSTRAINTS WALK shape criterion 5 (failure
# classification) will later give a class, but the reader itself (this criterion) must
# still behave sanely against it: no result, no silent success. Reads and discards the
# host's first stdin message like every sibling fixture, prints one `system/init` line,
# then exits.
self=$(readlink "$0") || self=$0
. "$(dirname "$self")/launch-args.sh"
sid=$(launched_session 22222222-2222-4222-8222-222222222222 "$@")
IFS= read -r line
echo '{"type":"system","subtype":"init","session_id":"'"$sid"'","model":"claude-haiku-4-5","mcp_servers":[]}'
