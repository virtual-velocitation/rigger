#!/bin/sh
# A fixture "agent" for the headless host's spawn configuration: it stands in for the real
# `claude` binary and records the exact argv it was started with, one NUL-terminated field per
# argument, to the file `$RIGGER_ARGV_CAPTURE` names (a path the test owns, outside the spawn's
# working directory), so a test can read back what really reached the child process - the
# settings, the MCP servers and the agent definitions a session would load. It then reads the
# host's first stream-json user message and exits. Checked in, never written at test time, for
# the same reason as its sibling fixtures.
: "${RIGGER_ARGV_CAPTURE:?the test names the argv capture file}"
for arg in "$@"; do
    printf '%s\0' "$arg"
done > "$RIGGER_ARGV_CAPTURE"
IFS= read -r line
echo "STDIN=$line"
