#!/bin/sh
# A fixture "agent" for periphery-testing Driver::read_stream's reap-on-error path (spec
# 104 criterion 2, adj-u104-stream REQUIRED FIX 1: "a genuine mid-stream read error" must
# still end the child through its own handle - ChildReaper - before spawn()'s `Err`
# propagates, not merely on a clean EOF or a missing result).
#
# Records its own pid FIRST (so the test can prove it does not linger, same rationale as
# claude-code-never-reads-stdin-agent.sh), reads and discards the host's first stream-json
# message like every sibling fixture (so launch()'s write never blocks), then writes ONE
# line of INVALID UTF-8 to stdout - `BufRead::lines()` on the host errors on that
# (`io::ErrorKind::InvalidData`), which is the READ failure this fixture exists to trigger,
# distinct from claude-code-stream-no-result-agent.sh's clean-EOF-with-no-result case.
# Finally sleeps far longer than any test waits, so a host that failed to reap this
# process would still find it alive when the test checks - deterministic either way, since
# the test only cares whether the process was killed, never how fast.
if [ -n "$RIGGER_TEST_INVALID_UTF8_PID_FILE" ]; then
    echo "$$" >"$RIGGER_TEST_INVALID_UTF8_PID_FILE"
fi
IFS= read -r line
printf '\377\376\n'
sleep 30
