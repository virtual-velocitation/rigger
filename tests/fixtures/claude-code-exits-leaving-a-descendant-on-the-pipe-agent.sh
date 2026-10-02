#!/bin/sh
# A fixture "agent" for THE STREAM's unbounded wake-up (spec 104 STOP; `UNBOUNDED_POLL`):
# a session that forks a background `sleep 60 &` (never `exec`'d, so it inherits this
# shell's stdout write end), then exits WITHOUT a result. The shell's own copy of the pipe
# closes, the descendant's does not, so the host never sees EOF. On an unbounded launch the
# only thing that notices the session is gone is the wake-up re-checking the child: it must
# end the read as a session with no result, never wait on the stranger's pipe forever.
# Reads and discards the host's first stdin message like every sibling fixture.
IFS= read -r line
echo '{"type":"system","subtype":"init","session_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","model":"claude-haiku-4-5","mcp_servers":[{"name":"rigger","status":"connected"}]}'
sleep 60 &
