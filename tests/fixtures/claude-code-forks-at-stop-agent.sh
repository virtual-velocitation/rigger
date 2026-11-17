#!/bin/sh
# A fixture "agent" whose processes start at the moments a stop or an end can miss them, the
# moment named by RIGGER_FORK_AT, for the table of stops and ends in
# tests/claude_code_stream_periphery.rs.
#
# Every process it starts holds the session's stdout and stderr and the write end of the witness
# FIFO (fd 9, opened from RIGGER_TEST_WITNESS), and each one that matters writes one byte there
# when it is up (`tests/common/witness.rs`). None exits on its own: each blocks reading the release
# FIFO (fd 8, opened from RIGGER_TEST_RELEASE) until the test drops that FIFO's only write end. So
# a process the stop or the end missed is still running when the test reads the witness, and the
# test's release, never a signal, ends it afterwards.
#
# The init line reports the rigger MCP server as RIGGER_TEST_MCP_STATUS (default `connected`):
# `failed` stops the session at that line as a launch fault.
#
# RIGGER_FORK_AT:
#   term             the TERM trap starts a holder; the session keeps idling
#   term-exit        the TERM trap starts a holder and the session exits
#   eof              at stdin EOF a holder starts; the session keeps idling
#   eof-exit         at stdin EOF a holder starts and the session exits
#   eof-then-margin  half a second after stdin EOF a holder starts; the session keeps idling
#   double           before init, a subshell starts a holder and exits at once, so the holder has
#                    left the session's process tree before any stop; the session exits at EOF
#   double-slow      after init, a subshell starts a holder and lives 3 s more before it exits;
#                    the session waits for it, then exits at stdin EOF
#   setsid           before init, a holder in a session of its own (util-linux setsid)
#   chdir            before init, a holder started after `cd /`
#   clean-exit       after init a holder starts, then the session prints its result and exits
self=$(readlink "$0") || self=$0
. "$(dirname "$self")/launch-args.sh"
sid=$(launched_session 99999999-9999-4999-8999-999999999999 "$@")
exec 9>"$RIGGER_TEST_WITNESS" 8<"$RIGGER_TEST_RELEASE"
printf u >&9
IFS= read -r line

# Blocks until the test releases the fixture.
held() {
    read -r unused <&8
}

# Starts a holder: a process that is up, holds what every session process holds, and runs until
# the test releases it.
holder() {
    (
        printf u >&9
        held
    ) &
}

# Idles until the test releases the fixture. A trapped signal interrupts only the wait, so a TERM
# trap runs at once; each idle child is a session process too.
idle() {
    while :; do
        (held) &
        wait $!
        [ $? -gt 128 ] || exit 0
    done
}

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
    init
    drain
    holder
    idle
    ;;
eof-exit)
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
double-slow)
    init
    (
        holder
        sleep 3
    )
    drain
    ;;
setsid)
    setsid sh -c 'printf u >&9; read -r unused <&8' &
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
