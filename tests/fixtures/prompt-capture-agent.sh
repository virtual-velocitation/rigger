#!/bin/sh
# A fixture "agent" for prompt delivery: it stands in for the real `claude` binary and copies
# what it was handed into the directory `$PROMPT_CAPTURE_DIR` names (a path the test owns,
# outside the spawn's scratch) - its stdin to `stdin`, and the file its
# `--system-prompt-file` argument names (found through `launch-args.sh`, sourced from
# beside this fixture's own path) to `system-prompt` (and that path itself to
# `system-prompt-path`) - so a test can read back that a
# prompt larger than the kernel's per-argument limit arrived intact. Each launch also goes on
# record under `launches` there (`launch-args.sh`'s `record_launch`), so a test reads back what
# every spawn of a run was started with, the spawns of a panel included. Both hosts drive it: under
# `--input-format` (the headless host, which keeps stdin open until a `result`) it takes the
# first user message and answers in stream-json; otherwise (the cli host, `rigger canary`) it
# takes stdin to its end and prints its answer. Checked in, never written at test time, for the
# same reason as its sibling fixtures.
: "${PROMPT_CAPTURE_DIR:?the test names the capture directory}"
self=$(readlink "$0") || self=$0
. "$(dirname "$self")/launch-args.sh"
record_launch "$PROMPT_CAPTURE_DIR/launches" "$@"
sid=$(launched_session capture "$@")
sp=$(system_prompt_file "$@")
stream=0
for a in "$@"; do
    if [ "$a" = "--input-format" ]; then
        stream=1
    fi
done
if [ "$stream" = "1" ]; then
    IFS= read -r line
    printf '%s\n' "$line" > "$PROMPT_CAPTURE_DIR/stdin"
else
    cat > "$PROMPT_CAPTURE_DIR/stdin"
fi
if [ -n "$sp" ]; then
    cat "$sp" > "$PROMPT_CAPTURE_DIR/system-prompt"
    printf '%s' "$sp" > "$PROMPT_CAPTURE_DIR/system-prompt-path"
fi
if [ "$stream" = "1" ]; then
    printf '%s\n' '{"type":"system","subtype":"init","session_id":"'"$sid"'","mcp_servers":[{"name":"rigger","status":"connected"}]}'
    printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"session_id":"capture","result":"{\"id\":\"final\",\"pass\":true}"}'
else
    echo '{"id":"final","pass":true}'
fi
