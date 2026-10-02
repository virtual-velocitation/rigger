#!/bin/sh
# A fixture "agent" for STOP (spec 104 criterion 6): reads and discards the host's first
# stdin message like every sibling fixture, prints one `system/init` line, then goes
# SILENT and stays alive - long enough for a test's short `max_wall_clock` to expire -
# but WINDS DOWN ON ITS OWN the moment the host closes its input (the exact "a session
# that notices its input closed and exits gracefully" case THE STOP's grace period
# exists for): the blocking `read` below returns non-zero on EOF, ending the loop and
# this script, well within a short test grace, so the host never needs to escalate to
# reap::end_child's SIGKILL.
IFS= read -r line
echo '{"type":"system","subtype":"init","session_id":"33333333-3333-4333-8333-333333333333","model":"claude-haiku-4-5","mcp_servers":[]}'
while IFS= read -r _unused; do :; done
