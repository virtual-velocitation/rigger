#!/bin/sh
# A fixture "agent" for STOP (spec 104 criterion 6 round-4 fix, decision
# op-104-stop-end-the-tree-and-bound-the-joins): the bounded-join backstop half of the
# SAME fix. Reads and discards the host's first stdin message, prints one `system/init`
# line, then DOUBLE-forks a background `sleep 60` out of the driven child's own process
# tree entirely - a subshell that backgrounds the sleep and then exits immediately (it has
# nothing left to do), so the sleep is reparented to init (or this system's nearest
# subreaper) well before THE STOP's descendant snapshot ever runs. Its pid is recorded
# from INSIDE that same subshell (the only place `$!` still names it) so a test can
# confirm it is NEVER touched by reap::end_child's pid-tree walk - it is not in the tree
# at all by the time that walk happens - while it still holds a copy of the stdout pipe's
# write end open. Winds down on its own the moment its own stdin closes, exactly like the
# in-tree sibling fixture.
IFS= read -r line
echo '{"type":"system","subtype":"init","session_id":"77777777-7777-4777-8777-777777777777","model":"claude-haiku-4-5","mcp_servers":[]}'
(
    sleep 60 &
    if [ -n "$RIGGER_TEST_DESCENDANT_PID_FILE" ]; then
        echo "$!" >"$RIGGER_TEST_DESCENDANT_PID_FILE"
    fi
)
while IFS= read -r _unused; do :; done
