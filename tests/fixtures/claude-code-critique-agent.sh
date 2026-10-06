#!/bin/sh
# A fixture "agent" for `rigger critique` (spec 112, criterion 1): stands in for the real
# `claude` binary, put first on PATH as a symlink named `claude` by
# `tests/common/repo.rs::write_critique_stub`. Everything it reads or writes lives beside that
# symlink, in the directory of `$0`: it records its argv (one NUL-terminated field per argument),
# the system prompt its `--system-prompt-file` names (read now, since the host removes that file
# once the spawn ends), the KURRENTDB_CONN it was started with (`unset` when none) and the host's
# first stream-json user message, appends one line to its spawn count, then replays the stream-json transcript the
# writer left there and exits. A checked-in executable
# fixture (never written at test time), for the same reason as every sibling fixture here: no
# concurrent test's fork+exec can inherit a write fd to it.
here=$(dirname "$0")
for arg in "$@"; do
    printf '%s\0' "$arg"
done > "$here/critique-argv"
sp=""
next=0
for arg in "$@"; do
    if [ "$next" = "1" ]; then
        sp="$arg"
        next=0
    fi
    if [ "$arg" = "--system-prompt-file" ]; then
        next=1
    fi
done
if [ -n "$sp" ]; then
    cat "$sp" > "$here/critique-system-prompt"
fi
printf '%s' "${KURRENTDB_CONN-unset}" > "$here/critique-conn"
IFS= read -r line
printf '%s\n' "$line" > "$here/critique-task.jsonl"
echo spawned >> "$here/critique-spawns"
cat "$here/critique-transcript.jsonl"
