#!/bin/sh
# A fixture "agent" for session continuity: it stands in for the real `claude` binary. Asked to
# `--resume` any session but the one `$SESSION_AGENT_KNOWN` names, it refuses exactly as Claude
# Code does when the session's transcript is gone (the tell on stderr, exit 1, no turn run);
# otherwise it echoes its argv and the task it read on stdin. Checked in, never written at test
# time, for the same reason as its sibling fixtures.
prev=""
for a in "$@"; do
    if [ "$prev" = "--resume" ] && [ "$a" != "${SESSION_AGENT_KNOWN:-}" ]; then
        echo "No conversation found with session ID: $a" >&2
        exit 1
    fi
    prev="$a"
done
echo "argv: $*"
echo "task: $(cat)"
