#!/bin/sh
# A fixture "agent" for periphery-testing Driver::launch's stdin-write-failure cleanup
# (spec 104 criterion 1: "a failure writing the first message ends the child through its
# own handle (kill() + wait()) before the error propagates"). Unlike
# claude-code-echo-agent.sh, this one NEVER reads its stdin - it records its own pid (so
# the test can later prove the process does not linger) and exits immediately.
#
# WHY THIS MATTERS: a parent write only fails once the pipe's read end is fully closed,
# which happens at child exit - racing that exit against the parent's write timing would
# be flaky. Never reading at all sidesteps the race: the test instead writes MORE than the
# pipe's kernel buffer can hold (the caller picks the size), so the write is forced to
# block until either more buffer space frees (it never does, since this script never
# reads) or every reader is gone (guaranteed, since this script exits almost immediately)
# - deterministic either way, regardless of how the write and the exit interleave.
#
# Checked in rather than written at test time, same rationale as claude-code-echo-agent.sh:
# no concurrent test's fork+exec can inherit a write fd to it.
if [ -n "$RIGGER_TEST_NEVER_READ_PID_FILE" ]; then
    echo "$$" >"$RIGGER_TEST_NEVER_READ_PID_FILE"
fi
exit 0
