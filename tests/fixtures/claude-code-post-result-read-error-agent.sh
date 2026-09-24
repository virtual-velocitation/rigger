#!/bin/sh
# A fixture "agent" for periphery-testing Driver::read_stream's post-result read-error
# path (spec 104 criterion 2 round 3, adj-u104-stream round-2 REQUIRED FIX: "once
# `result` is `Some`, a later per-line read error must not overturn the durably-recorded
# success"). Replays the SAME recorded success stream every sibling fixture uses (so the
# real `result` line is durably recorded through `record_result_if_absent` exactly once,
# the same way `claude-code-stream-agent.sh` proves a clean run), THEN - after that
# `result` line has already gone out - writes ONE line of INVALID UTF-8
# (`BufRead::lines()` errors on this the same way `claude-code-invalid-utf8-agent.sh`
# does). The error strikes strictly AFTER the result, the opposite order from that
# sibling fixture, which errors before any result is ever produced.
#
# Records its own pid first (same rationale as claude-code-invalid-utf8-agent.sh) so the
# test can independently confirm the child is still reaped through its own handle on
# this path too, never left running, then `exec`s into a long sleep - replacing this
# shell's own process image rather than forking a child for it - so a host that failed
# to reap it would still find the SAME pid alive when the test checks, and so a host
# that DOES reap it (kill() on this exact pid) leaves no separate orphaned sleep behind
# still holding stdout/stderr open (a plain trailing `sleep 30` forks a child dash does
# NOT tail-call-optimize away here, which would keep this fixture's stderr pipe open -
# and `read_stream`'s stderr-drain thread blocked - for the full 30s even after this
# script's own pid is correctly killed).
if [ -n "$RIGGER_TEST_POST_RESULT_PID_FILE" ]; then
    echo "$$" >"$RIGGER_TEST_POST_RESULT_PID_FILE"
fi
IFS= read -r line
cat "$(dirname "$0")/claude-code-stream-success.jsonl"
printf '\377\376\n'
exec sleep 30
