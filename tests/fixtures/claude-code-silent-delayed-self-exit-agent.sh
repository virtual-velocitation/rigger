#!/bin/sh
# A fixture "agent" pinning THE STOP's grace-period LOOP itself (spec 104 criterion 6),
# distinct from `claude-code-silent-winds-down-on-eof-agent.sh`: that sibling exits so
# fast on EOF (its own blocking `read` returning immediately) that a host which skips the
# grace-wait ENTIRELY - the deadline computed behind `now()`, or the loop's own bound
# check inverted so it never iterates - still passes, because there is nothing for a
# skipped wait to have raced against. This one instead sleeps a beat past its own EOF
# before exiting, standing in for a session that takes a MOMENT to wind itself down once
# its input closes - long enough that a host with a genuinely broken (zero-iteration)
# grace loop reaches `reap::end_child`'s SIGTERM before this script ever gets there,
# killing it mid-sleep; only a host that actually POLLS across the grace window observes
# the exit on its own and lets it finish. The marker is written only once that sleep
# completes, so a live host's own `try_wait` timing decides its fate.
IFS= read -r line
echo '{"type":"system","subtype":"init","session_id":"66666666-6666-4666-8666-666666666666","model":"claude-haiku-4-5","mcp_servers":[]}'
while IFS= read -r _unused; do :; done
sleep 0.15
if [ -n "$RIGGER_TEST_SELF_EXIT_MARKER" ]; then
    touch "$RIGGER_TEST_SELF_EXIT_MARKER"
fi
