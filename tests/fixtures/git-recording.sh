#!/bin/sh
# A fixture `git` for the rebuild's periphery tests (spec 107, criterion 5): put first on PATH as
# a symlink named `git` by `tests/common/repo.rs::stub_path`, it appends its arguments as one
# line to `git-invocations` beside that symlink, in the directory of `$0`, then becomes the real
# git found on the rest of PATH, so every answer is git's own. A checked-in executable fixture
# (never written at test time), for the same reason as every sibling fixture here: no concurrent
# test's fork+exec can inherit a write fd to it.
here=$(dirname "$0")
printf '%s\n' "$*" >> "$here/git-invocations"
PATH=${PATH#"$here":}
if [ "$(command -v git)" = "$here/git" ]; then
    echo "git-recording.sh: no git on PATH past $here" >&2
    exit 127
fi
exec git "$@"
