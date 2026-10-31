#!/bin/sh
# A fixture "agent" for the end-to-end `rigger run` tests: stands in for the real `claude`
# binary, put first on PATH as a symlink named `claude` by `tests/cli.rs::install_fake_claude`,
# and speaks the stream-json contract of the headless host. Beside the symlink, in the directory
# of `$0`, the test left `claude-arms`: a shell `case` over `$sp`, the system prompt its
# `--system-prompt-file` names. The arm runs in a subshell with the launch's arguments, its
# first user message in `$first` and the directory in `$here`, so it may record any of them
# there; what it prints is the session's answer. The stand-in reports the spawn's `rigger` server
# connected, then a `result` carrying that answer as a JSON string. An arm that exits non-zero
# ends the session with its status and no `result`. A checked-in executable fixture (never
# written at test time), for the same reason as every sibling fixture here: no concurrent test's
# fork+exec can inherit a write fd to it.
here=$(dirname "$0")
sp=""
next=0
for a in "$@"; do
    if [ "$next" = "1" ]; then
        sp=$(cat "$a")
        next=0
    fi
    if [ "$a" = "--system-prompt-file" ]; then
        next=1
    fi
done
IFS= read -r first
answer=$(. "$here/claude-arms")
status=$?
[ "$status" = 0 ] || exit "$status"
tab=$(printf '\t')
json=$(printf '%s' "$answer" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' -e "s/$tab/\\\\t/g" |
    awk 'NR > 1 { printf "%s", "\\n" } { printf "%s", $0 }')
printf '%s\n' '{"type":"system","subtype":"init","session_id":"arms","mcp_servers":[{"name":"rigger","status":"connected"}]}'
printf '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"session_id":"arms","result":"%s"}\n' "$json"
