#!/bin/sh
# A fixture "agent" for periphery-testing that Driver::read_stream drains stderr
# CONCURRENTLY with stdout (spec 104 criterion 2, adj-u104-stream REQUIRED FIX 2: sdet's
# stderr-flood fixture, reproduced against the compiled driver in the rejected round).
#
# Reads and discards the host's first stream-json message like every sibling fixture, then
# writes 1MiB to STDERR - far more than a pipe's kernel buffer (Linux default 64KiB) - so a
# host that does not read stderr on its own thread would block this write forever, starving
# the stdout loop of the recorded stream below it would otherwise never get to send. Only
# once that write completes does it replay the same recorded stream
# claude-code-stream-agent.sh does, so a host that DOES drain stderr concurrently still
# gets a normal, real result rather than this fixture existing purely to hang.
IFS= read -r line
yes x | head -c 1048576 >&2
echo >&2
cat "$(dirname "$0")/claude-code-stream-success.jsonl"
