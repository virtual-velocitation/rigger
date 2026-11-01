#!/bin/sh
# A fixture "agent" for THE RESUME FALLBACK (spec 105 criterion 1): stands in for the real
# `claude` binary and ends the way its test names. A launch carrying `--resume` ends as
# `$RIGGER_TEST_RESUME_ENDING`, any other as `$RIGGER_TEST_FRESH_ENDING`, unset reading `answer`.
# `refused-unread` exits without reading the host's first stdin message; every other ending
# reads it and, when `$RIGGER_TEST_LAUNCH_LOG` names a file, appends it there, one line a launch.
#
# `refused-result` is the literal stdout of Claude Code 2.1.314 asked to `--resume` a session it
# holds no transcript of, probed 2026-11-01: one `result` line, no `system/init`, the tell on
# stderr too, exit 1. `refused-eof` is the same refusal with nothing on stdout, the shape of
# Claude Code 2.1.290. Each `retried-*` ending prints one `api_retry` line of
# `$RIGGER_TEST_RETRY_CATEGORY` and no `system/init`, then ends as `refused-eof`, `silent` or
# `unreadable` does. A checked-in executable fixture (never written at test time), for the same
# reason as every sibling fixture here: no concurrent test's fork+exec can inherit a write fd to
# it.
case " $* " in
*" --resume "*) ending=${RIGGER_TEST_RESUME_ENDING:-answer} ;;
*) ending=${RIGGER_TEST_FRESH_ENDING:-answer} ;;
esac

refusal_tell() {
    echo 'No conversation found with session ID: sess-prior' >&2
}

wait_for_input_to_close() {
    while IFS= read -r _unused; do :; done
}

init() {
    printf '%s\n' "{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"99999999-9999-4999-8999-999999999999\",\"model\":\"claude-haiku-4-5\",\"mcp_servers\":[$1]}"
}

retried() {
    printf '%s\n' "{\"type\":\"system\",\"subtype\":\"api_retry\",\"attempt\":1,\"max_retries\":10,\"retry_delay_ms\":1,\"error_status\":500,\"error\":\"$RIGGER_TEST_RETRY_CATEGORY\"}"
}

refused_result() {
    echo '{"type":"result","subtype":"error_during_execution","duration_ms":0,"duration_api_ms":0,"is_error":true,"num_turns":0,"stop_reason":null,"session_id":"sess-prior","total_cost_usd":0,"usage":{"input_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":0},"modelUsage":{},"permission_denials":[],"errors":["No conversation found with session ID: sess-prior"],"result_index":0}'
}

answer() {
    printf '%s\n' "{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"num_turns\":1,\"result\":\"${RIGGER_TEST_ANSWER:-the answer}\",\"session_id\":\"99999999-9999-4999-8999-999999999999\",\"total_cost_usd\":0.01,\"usage\":{\"input_tokens\":10,\"output_tokens\":5},\"permission_denials\":[]}"
}

if [ "$ending" = refused-unread ]; then
    refusal_tell
    exit 1
fi

IFS= read -r line
if [ -n "$RIGGER_TEST_LAUNCH_LOG" ]; then
    printf '%s\n' "$line" >>"$RIGGER_TEST_LAUNCH_LOG"
fi

case "$ending" in
answer)
    init
    answer
    ;;
answer-no-init)
    answer
    ;;
refused-result)
    refusal_tell
    refused_result
    exit 1
    ;;
refused-result-held-open)
    refused_result
    wait_for_input_to_close
    exit 1
    ;;
refused-eof)
    refusal_tell
    exit 1
    ;;
silent)
    wait_for_input_to_close
    exit 1
    ;;
unreadable)
    printf '\377\376\n'
    exec sleep 30
    ;;
retried-eof)
    retried
    exit 1
    ;;
retried-silent)
    retried
    wait_for_input_to_close
    exit 1
    ;;
retried-unreadable)
    retried
    printf '\377\376\n'
    exec sleep 30
    ;;
init-eof)
    init
    exit 1
    ;;
init-silent)
    init
    wait_for_input_to_close
    exit 1
    ;;
init-unreadable)
    init
    printf '\377\376\n'
    exec sleep 30
    ;;
init-server-failed)
    init '{"name":"rigger","status":"failed"}'
    wait_for_input_to_close
    exit 1
    ;;
*)
    echo "claude-code-resume-ending-agent: unknown ending: $ending" >&2
    exit 2
    ;;
esac
