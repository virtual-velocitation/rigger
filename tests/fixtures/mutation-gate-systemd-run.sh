#!/bin/sh
# A fixture standing in for `systemd-run` under the check-in mutation gate: it appends its own
# argv (one line per invocation, space-joined) to `$RIGGER_SCOPE_CAPTURE`, then runs the command
# after `--` in place, exactly as `systemd-run --scope` runs its command in the caller's
# environment. Checked in for the same reason as mutation-gate-cargo.sh.
: "${RIGGER_SCOPE_CAPTURE:?the test names the scope capture file}"
printf '%s ' "$@" >> "$RIGGER_SCOPE_CAPTURE"
echo >> "$RIGGER_SCOPE_CAPTURE"
while [ "$#" -gt 0 ] && [ "$1" != "--" ]; do
    shift
done
shift
exec "$@"
