#!/bin/sh
# The token count: the hand-over's instrument, NOT a gate - it is wired into no stage of
# .rigger/workflow.yml. It prints `?: N, return: N, break: N, continue: N` over the production
# hunks of `git diff -U0 $(git merge-base rigger-run HEAD)..HEAD -- crates src`: `.rs` files
# only, files under a `tests/` directory and everything from a file's column-0 `#[cfg(test)]`
# module onward (read from `git show HEAD:<file>`) left out, string literals and `//` comments
# stripped. A `?` counts when `;`, `.`, `)`, `,`, `]`, a space or the line's end follows it, so
# `?Sized`, `{x:?}` and a SQL `?1` do not. The author's probe record carries these counts in its
# tally lines and the `sdet` lens re-runs this script on the review tree.
#
# Usage: sh .rigger/gates/token-count.sh   (from a worktree root)

git diff -U0 "$(git merge-base rigger-run HEAD)"..HEAD -- crates src | awk '
/^\+\+\+ / { f = substr($2, 3); t = 0; if (f ~ /\.rs$/ && f !~ /(^|\/)tests\//) { c = "git show HEAD:" f; n = 0; p = ""; while ((c | getline l) > 0) { n++; if (!t && p ~ /^#\[cfg\(test\)\]/ && l ~ /^(pub(\([a-z]+\))? )?mod [^;]*[{]/) t = n; p = l } close(c) } next }
/^@@/ { split($3, a, ","); ln = substr(a[1], 2) + 0; next }
/^\+/ { if (f ~ /\.rs$/ && f !~ /(^|\/)tests\// && (!t || ln < t)) { s = substr($0, 2); gsub(/"([^"\\]|\\.)*"/, "", s); sub(/\/\/.*/, "", s); q += gsub(/\?([;.),\] ]|$)/, "", s); r += gsub(/(^|[^A-Za-z0-9_])return([^A-Za-z0-9_]|$)/, " ", s); b += gsub(/(^|[^A-Za-z0-9_])break([^A-Za-z0-9_]|$)/, " ", s); k += gsub(/(^|[^A-Za-z0-9_])continue([^A-Za-z0-9_]|$)/, " ", s) } ln++ }
END { print "?: " q + 0 ", return: " r + 0 ", break: " b + 0 ", continue: " k + 0 }'
