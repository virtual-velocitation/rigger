#!/bin/sh
# A fixture "agent" for periphery-testing Driver::read_stream's pin-to-the-FIRST-result
# guard (spec 104 criterion 2 round 4, adj-u104-stream round-3 REQUIRED FIX 1: "a genuine
# SECOND result-type line must never overwrite the already-captured result"). Replays the
# SAME recorded success stream every sibling fixture uses (so the real, FIRST `result`
# line is durably recorded through `record_result_if_absent` exactly once, precisely as
# `claude-code-stream-agent.sh` proves a clean run), THEN - after that result line has
# already gone out and closed stdin's write side is irrelevant here (the host, not this
# fixture, closes ITS OWN stdin to the child on the first result; this fixture keeps
# talking regardless, exactly like a real duplicate-result transcript would) - emits a
# SECOND, well-formed `result` message carrying deliberately different values in every
# field the reader maps, then exits cleanly (a clean EOF, unlike the sibling
# read-error-after-a-result fixture, which is a different shape entirely: a bad line, not
# a genuine second result).
IFS= read -r line
cat "$(dirname "$0")/claude-code-stream-success.jsonl"
echo '{"type":"result","subtype":"success","is_error":false,"duration_ms":50,"duration_api_ms":40,"num_turns":9,"result":"SECOND result - must never win","session_id":"22222222-2222-4222-8222-222222222222","total_cost_usd":9.99,"usage":{"input_tokens":999,"output_tokens":999,"cache_creation_input_tokens":999,"cache_read_input_tokens":999},"permission_denials":[]}'
