#!/bin/sh
# A fixture "agent" for the end-to-end `rigger run` tests: stands in for the real `claude`
# binary, put first on PATH as a symlink named `claude` by `tests/cli.rs::install_fake_claude`,
# and speaks the stream-json contract of the headless host. Beside the symlink, in the directory
# of `$0`, the test left `claude-arms`: a shell `case` over `$sp`, the system prompt its
# `--system-prompt-file` names. The arm runs in the stand-in's own shell, never a subshell, with
# the launch's arguments, its first user message in `$first` and the directory in `$here`, so it
# may record any of them there; what it prints is the session's answer, held in `reply.<pid>`
# beside the stand-in until the arm ends, and what it prints to fd 3 reaches the host as raw
# stream lines ahead of the rest. When the arm ends, or exits 0, the stand-in reports the spawn's
# `rigger` server connected, then a `result` carrying that answer as a JSON string. An arm that
# exits non-zero ends the session with its status and no `result`. An arm that `exec`s ends the
# session as what it execs, the very process the host spawned and reaps, with no `result`, and
# leaves its `reply.<pid>` beside the stand-in. A
# launch whose first argument is `auth` (the `claude auth status --json` read) prints the arm's
# answer as it stands, with no stream framing. A launch naming no `--system-prompt-file` (the
# hold probe's canary) reaches the arm for the empty prompt, `""`. A checked-in executable
# fixture (never written at test time), for the same reason as every sibling fixture here: no
# concurrent test's fork+exec can inherit a write fd to it.
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
verb=$1
reply="$here/reply.$$"
exec 3>&1
frame() {
    status=$?
    answer=$(cat "$reply")
    rm -f "$reply"
    [ "$status" = 0 ] || exit "$status"
    if [ "$verb" = auth ]; then
        printf '%s\n' "$answer" >&3
        exit 0
    fi
    tab=$(printf '\t')
    json=$(printf '%s' "$answer" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' -e "s/$tab/\\\\t/g" |
        awk 'NR > 1 { printf "%s", "\\n" } { printf "%s", $0 }')
    printf '%s\n' '{"type":"system","subtype":"init","session_id":"arms","mcp_servers":[{"name":"rigger","status":"connected"}]}' >&3
    printf '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"session_id":"arms","result":"%s"}\n' "$json" >&3
}
trap frame EXIT
. "$here/claude-arms" > "$reply"
