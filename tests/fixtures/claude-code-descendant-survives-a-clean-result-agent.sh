#!/bin/sh
# A fixture "agent" for THE STREAM's own bounded joins (spec 104 criterion 6 round-4 fix,
# decision op-104-stop-end-the-tree-and-bound-the-joins): that fix's bounded-join helper
# (Driver::join_within_grace) sits at FOUR reader/drain join call sites, not just the two
# THE STOP itself takes - read_stream's own ORDINARY exit (a real result lands, no
# wall-clock silence, THE STOP never runs at all) hits the identical two joins right after
# its own single-child reap (dash::ReapedChild::drop, which ends only the one held child,
# never walks a process tree the way reap::end_child does for THE STOP). This fixture
# proves that half of the fix: it forks a background `sleep 60 &` (never `exec`'d, so it
# inherits this shell's stdout AND stderr write ends) BEFORE emitting its result line,
# then exits normally right after - the shell's own copies of both pipes close on exit,
# but the descendant's inherited copies do not, so a genuine result lands and the host
# must still return promptly rather than block forever in either reader/drain thread's
# join waiting for an EOF only the descendant can still withhold.
IFS= read -r line
echo '{"type":"system","subtype":"init","session_id":"88888888-8888-4888-8888-888888888888","model":"claude-haiku-4-5","mcp_servers":[]}'
sleep 60 &
if [ -n "$RIGGER_TEST_DESCENDANT_PID_FILE" ]; then
    echo "$!" >"$RIGGER_TEST_DESCENDANT_PID_FILE"
fi
echo '{"type":"result","subtype":"success","is_error":false,"duration_ms":100,"duration_api_ms":80,"num_turns":1,"result":"done: a result survives a still-open descendant pipe","session_id":"88888888-8888-4888-8888-888888888888","total_cost_usd":0,"usage":{"input_tokens":1,"output_tokens":1,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"permission_denials":[]}'
