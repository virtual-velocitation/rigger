#!/bin/sh
# A fixture "agent" for THE STREAM (spec 104 criterion 2): stands in for the real
# `claude` binary end to end. It reads (and discards) the host's first stream-json
# user message on stdin - exactly like criterion 1's echo fixture, so the host's
# `launch()` write never blocks on a full pipe - then replays a checked-in recorded
# stream (`claude-code-stream-success.jsonl`, grounded on a real
# `claude -p --output-format stream-json` session) to stdout and exits. A checked-in
# executable fixture (never written at test time), matching every sibling fixture's own
# rationale in this directory: no concurrent test's fork+exec can inherit a write fd to
# it.
IFS= read -r line
cat "$(dirname "$0")/claude-code-stream-success.jsonl"
