#!/bin/sh
# A fixture "agent" for STOP (spec 104 criterion 6 round-4 fix, decision
# op-104-stop-end-the-tree-and-bound-the-joins): reproduces the reopened hang the round-3
# adjudication upheld - a descendant the driven child forked but never `exec`'d inherits
# the stdout pipe's write end, so ending only the directly-held child never closes that
# inherited copy. Reads and discards the host's first stdin message like every sibling
# fixture, prints one `system/init` line, forks a background `sleep 60` (`&`, never
# `exec`'d - so it stays a genuine CHILD of THIS shell, still inside the driven child's
# own process tree) that inherits this shell's own stdout fd and records that
# descendant's pid, THEN goes silent and IGNORES its input closing entirely (mirrors
# claude-code-silent-ignores-eof-agent.sh's own `exec sleep` tail - see that fixture's
# comment for why `exec`, never a plain `sleep` subprocess, replaces THIS shell's own
# image): a parent that instead wound down gracefully the moment stdin closed would race
# its own exit against the host's descendant snapshot - reparenting `sleep 60` away
# within microseconds of EOF, before the host's very next statement could ever see it -
# so this fixture stays alive (now as the exec'd `sleep`) regardless of stdin, giving the
# host's pid-tree walk a reliable window in which the descendant is still genuinely part
# of its process tree, and exercising the SAME TERM-then-grace-then-KILL escalation on
# the parent that `spawn_escalates_to_the_sanctioned_reap_when_a_silent_child_ignores_its_input_closing`
# already proves for a childless fixture.
IFS= read -r line
echo '{"type":"system","subtype":"init","session_id":"66666666-6666-4666-8666-666666666666","model":"claude-haiku-4-5","mcp_servers":[]}'
sleep 60 &
if [ -n "$RIGGER_TEST_DESCENDANT_PID_FILE" ]; then
    echo "$!" >"$RIGGER_TEST_DESCENDANT_PID_FILE"
fi
exec sleep 300
