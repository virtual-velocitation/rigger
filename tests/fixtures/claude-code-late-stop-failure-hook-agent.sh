#!/bin/sh
# A fixture "agent" whose turn ends on a rate limit: it starts the StopFailure hook its settings
# wire for `rate_limit` fire and forget, as Claude Code runs that hook, the hook's streams its own,
# and exits with no result, so the hook is still writing its record when the session's main has
# exited.
self=$(readlink "$0") || self=$0
. "$(dirname "$self")/launch-args.sh"
sid=$(launched_session 77777777-7777-4777-8777-777777777777 "$@")
IFS= read -r line
echo '{"type":"system","subtype":"init","session_id":"'"$sid"'","model":"claude-haiku-4-5","mcp_servers":[]}'
hook=$(launch_arg --settings "$@" | sed -n 's/.*"command": *"\([^"]*--class rate_limit\)".*/\1/p')
sh -c "$hook" </dev/null >/dev/null 2>&1 &
