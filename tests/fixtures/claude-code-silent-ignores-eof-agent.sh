#!/bin/sh
# A fixture "agent" for STOP (spec 104 criterion 6): the escalation case. Reads and
# discards the host's first stdin message, prints one `system/init` line, records its
# own pid (so a test can later prove it stopped existing), then goes SILENT and IGNORES
# its input closing entirely - it never reads stdin again and never exits on its own -
# so the host MUST escalate past the grace period to reap::end_child's SIGKILL fallback
# to end it. `exec sleep` (never a plain `sleep` subprocess): replaces THIS shell's own
# image in place, so the process the host holds a `Child` handle to (and signals) IS the
# sleeping process itself - no orphaned grandchild is left holding a duplicate of the
# stdout pipe fd open after the signalled process exits, which would otherwise strand
# the host's reader thread blocked on EOF for the sleep's own full duration regardless
# of the signal succeeding (a real `claude` binary has no such shell-wrapper layer).
IFS= read -r line
echo '{"type":"system","subtype":"init","session_id":"44444444-4444-4444-8444-444444444444","model":"claude-haiku-4-5","mcp_servers":[]}'
if [ -n "$RIGGER_TEST_IGNORES_EOF_PID_FILE" ]; then
    echo "$$" >"$RIGGER_TEST_IGNORES_EOF_PID_FILE"
fi
exec sleep 300
