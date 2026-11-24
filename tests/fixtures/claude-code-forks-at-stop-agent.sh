#!/bin/sh
# A fixture "agent" whose processes start at the moments a stop or an end can miss them, the
# moment named by RIGGER_FORK_AT, for the table of stops and ends in
# tests/claude_code_stream_periphery.rs.
#
# Every process it starts holds the session's stdout and stderr and the witness's two FIFOs
# (`witness.sh`, beside this file): each one that matters writes its up byte, and none exits until
# the test releases it, never by a signal of the test's own.
#
# The init line reports the rigger MCP server as RIGGER_TEST_MCP_STATUS (default `connected`):
# `failed` stops the session at that line as a launch fault.
#
# RIGGER_FORK_AT:
#   term             the TERM trap starts a holder; the session keeps idling
#   term-exit        the TERM trap starts a holder and the session exits
#   eof              at stdin EOF a holder starts; the session keeps idling
#   eof-exit         at stdin EOF a holder starts and the session exits
#                    (both ignore TERM, so an end's TERM round finds the session still reading its
#                    input and starting its holder, which ignores TERM too and is ended by KILL)
#   eof-then-margin  half a second after stdin EOF a holder starts; the session keeps idling
#   double           before init, a subshell starts a holder and exits at once, so the holder has
#                    left the session's process tree before any stop; the session exits at EOF
#   daemon           `double` from `/`, as daemon(3) leaves a process: out of the session's process
#                    tree and out of its working directory, where no sweep of that directory sees it
#   double-slow      after init, a subshell starts a holder and lives 3 s more before it exits;
#                    the session waits for it, then exits at stdin EOF
#   setsid           before init, a holder in a session of its own (util-linux setsid)
#   chdir            before init, a holder started after `cd /`
#   clean-exit       after init a holder starts, then the session prints its result and exits
self=$(readlink "$0") || self=$0
. "$(dirname "$self")/launch-args.sh"
answer_version "$@"
sid=$(launched_session 99999999-9999-4999-8999-999999999999 "$@")
. "$(dirname "$self")/witness.sh"
up
IFS= read -r line

# Reads the session's input to its end.
drain() {
    while IFS= read -r unused; do :; done
}

init() {
    echo '{"type":"system","subtype":"init","session_id":"'"$sid"'","model":"claude-haiku-4-5","mcp_servers":[{"name":"rigger","status":"'"${RIGGER_TEST_MCP_STATUS:-connected}"'"}]}'
}

case "$RIGGER_FORK_AT" in
term)
    trap holder TERM
    init
    idle
    ;;
term-exit)
    trap 'holder; exit 0' TERM
    init
    idle
    ;;
eof)
    trap '' TERM
    init
    drain
    holder
    idle
    ;;
eof-exit)
    trap '' TERM
    init
    drain
    holder
    ;;
eof-then-margin)
    init
    drain
    sleep 0.5
    holder
    idle
    ;;
double)
    (holder)
    init
    drain
    ;;
daemon)
    (cd / && holder)
    init
    drain
    ;;
double-slow)
    init
    (
        holder
        sleep 3
    )
    drain
    ;;
setsid)
    holder setsid
    init
    idle
    ;;
chdir)
    cd / && holder
    init
    idle
    ;;
clean-exit)
    init
    holder
    echo '{"type":"result","subtype":"success","is_error":false,"duration_ms":100,"duration_api_ms":80,"num_turns":1,"result":"done: a result survives a still-open descendant pipe","session_id":"99999999-9999-4999-8999-999999999999","total_cost_usd":0,"usage":{"input_tokens":1,"output_tokens":1,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"permission_denials":[]}'
    ;;
*)
    echo "claude-code-forks-at-stop-agent.sh: unknown RIGGER_FORK_AT '$RIGGER_FORK_AT'" >&2
    exit 2
    ;;
esac
