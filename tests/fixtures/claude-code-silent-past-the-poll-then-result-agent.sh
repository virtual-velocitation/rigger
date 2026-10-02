#!/bin/sh
# A fixture "agent" for THE STOP's bound (spec 104 STOP: "the host enforces
# `max_wall_clock` against stream silence"; 0 is unbounded): a session that thinks in
# silence for longer than the host's unbounded wake-up interval (`UNBOUNDED_POLL`, 5 s) and
# then produces its result - the ordinary shape of a real agent's long first turn. An
# unbounded launch must record that result; a bounded one still stops at its wall clock.
# Reads and discards the host's first stdin message like every sibling fixture.
IFS= read -r line
echo '{"type":"system","subtype":"init","session_id":"99999999-9999-4999-8999-999999999999","model":"claude-haiku-4-5","mcp_servers":[{"name":"rigger","status":"connected"}]}'
sleep 7
echo '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"done: thought past the poll","session_id":"99999999-9999-4999-8999-999999999999","total_cost_usd":0,"usage":{"input_tokens":1,"output_tokens":1},"permission_denials":[]}'
