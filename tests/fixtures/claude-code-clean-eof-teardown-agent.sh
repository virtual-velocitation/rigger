#!/bin/sh
# A fixture "agent" pinning THE STREAM's clean-EOF reap (spec 104 criterion 2 round 5,
# adj-u104-stream round-4 REQUIRED FIX / op-104-stream-clean-eof-waits-before-the-reap):
# "a clean EOF must wait for the child to actually exit before the reap ever considers a
# signal, not the non-blocking try_wait-then-kill round-4 fell back to on the ordinary
# still-running case." Replays the SAME recorded success stream every sibling fixture
# uses (so the real result line is durably recorded exactly like
# `claude-code-stream-agent.sh` proves a clean run), THEN closes its OWN stdout
# (`exec >&-`) while staying alive to do legitimate post-output teardown - a brief sleep,
# standing in for flushing telemetry or releasing a lock - and writes its completion
# marker only AFTER that teardown finishes. A host that force-ends this process the
# instant its stdout pipe reads EOF (the round-4 regression) reaps it mid-teardown, so
# the marker is never written; a host that instead waits for the process to exit on its
# own (the round-5 fix) always finds it there.
IFS= read -r line
cat "$(dirname "$0")/claude-code-stream-success.jsonl"
exec >&-
sleep "${RIGGER_TEST_TEARDOWN_SLEEP:-1}"
if [ -n "$RIGGER_TEST_TEARDOWN_MARKER" ]; then
    touch "$RIGGER_TEST_TEARDOWN_MARKER"
fi
