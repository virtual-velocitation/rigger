#!/bin/sh
# A fixture "agent" for `rigger critique` (spec 112, criterion 1; spec 123's shared stub): stands
# in for the real `claude` binary, put first on PATH as a symlink named `claude` by
# `tests/common/repo.rs::write_critique_stub`. Everything it records or replays lives beside that
# symlink, in the directory of `$0`. On its first spawn - its spawn count still empty - it records
# its argv (one NUL-terminated field per argument), the system prompt its `--system-prompt-file`
# names (found through `launch-args.sh`, sourced from beside this fixture's own path, and read
# now, since the host removes that file once the spawn ends), the KURRENTDB_CONN it was started
# with (`unset` when none) and the host's first stream-json user message, then replays the
# canned transcript the writer left there. Every later spawn records none of those: it still
# reads the host's first message, then replays the later transcript the writer left beside the
# canned one. While a `critique-hold` file stands beside it, a spawn that has read the host's
# first message writes `critique-held` and waits (bounded, for a test that panicked) until the
# hold is removed, so a test acts while that pass is in flight. Each spawn appends one line to
# its spawn count before it replays. A checked-in executable fixture (never written at test
# time), for the same reason as every sibling fixture here: no concurrent test's fork+exec can
# inherit a write fd to it.
here=$(dirname "$0")
self=$(readlink "$0") || self=$0
. "$(dirname "$self")/launch-args.sh"
sid=$(launched_session "" "$@")
if [ -s "$here/critique-spawns" ]; then
    transcript="$here/critique-later-transcript.jsonl"
    IFS= read -r line
else
    transcript="$here/critique-transcript.jsonl"
    for arg in "$@"; do
        printf '%s\0' "$arg"
    done > "$here/critique-argv"
    sp=$(system_prompt_file "$@")
    if [ -n "$sp" ]; then
        cat "$sp" > "$here/critique-system-prompt"
    fi
    printf '%s' "${KURRENTDB_CONN-unset}" > "$here/critique-conn"
    IFS= read -r line
    printf '%s\n' "$line" > "$here/critique-task.jsonl"
fi
if [ -e "$here/critique-hold" ]; then
    : > "$here/critique-held"
    waited=0
    while [ -e "$here/critique-hold" ] && [ "$waited" -lt 600 ]; do
        sleep 0.1
        waited=$((waited + 1))
    done
fi
echo spawned >> "$here/critique-spawns"
as_launched "$sid" <"$transcript"
