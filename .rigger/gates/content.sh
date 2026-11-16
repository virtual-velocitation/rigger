#!/bin/sh
# The content gates: checks over the lines a diff ADDS, each failing its gate when an added line
# carries a forbidden shape, and passing on a clean diff.
#
#   style (Gap 4a): a mechanically-checkable style constraint as a gate rather than ambient
#     reviewer attention - no em dash (U+2014) in any added text: code, comments, docs, prompts.
#     The byte pattern is generated at runtime via `printf` octal (portable to POSIX sh / dash and
#     bash), so this script carries no literal em dash and passes on its own diff.
#   no-os-kill (spec 78): rigger issues NO OS-level kills. Process lifecycle is handle-bound
#     (`Child::kill()`/`wait()` on a child this process spawned) or, in exactly two sanctioned
#     places, an internal guarded signal call: the worktree reaper
#     (crates/rigger-process/src/reap.rs) and the shared test lifecycle helper
#     (tests/common/mod.rs). Everywhere else in src/, crates/ and tests/, an added line that shells
#     out to kill/pkill/killall/xkill/taskkill (the name in any case), formats a negative
#     (process-group) pid, uses the `--` argv separator, or calls a signal API directly FAILS.
#     Inside the two sanctioned files a shell-out is still a failure. The Windows
#     process-termination call (TerminateProcess) has one site, the reaper's send_signal: an added
#     call anywhere else, the test helper included, FAILS. Scoped to code: prose in docs/, specs/
#     and .rigger/ may name the rule.
#     WHY: computed kill targets (a pgid, a marker pid, a /proc scan) resolved to kill(-1) and
#     destroyed the operator's whole desktop session seven-plus times; the review panel approved
#     each site because nothing mechanical forbade it. Now something does.
#   no-new-event-type: the event log's vocabulary stays closed. An event type is a
#     `const TYPE_<NAME>: &str` in src/ or crates/, and its value is its name in the log. A
#     declaration the diff adds passes when the base tree, or the lines the diff removes, already
#     declare its value (a move, a rename, a second declaration) or the type it re-exports;
#     one with any other value FAILS as a new event type, and a new value for an existing name
#     FAILS as a re-valued one, naming it either way. A new fact rides an existing event type,
#     as a status or a meta key, so every reader of the log, an older store's included, already
#     knows its type. In-crate test modules are covered like any other code; only the
#     integration suites under tests/ may declare their own. An event type no `TYPE_` const
#     names (a literal, a const of another name) is the review panel's to find.
#
# Usage: sh .rigger/gates/content.sh <style|no-os-kill|no-new-event-type> [base]
# Which diff is checked is the caller's stage, and both forms take a three-dot diff to HEAD:
#   no base - a unit's own changes: from the run branch (`rigger-run`, with an origin/main / HEAD
#     fallback), run in the unit worktree, so it reports only THIS branch's changes, never the
#     whole tree.
#   base - everything since `base`: the check-in stage passes "$RIGGER_RUN_BASE" (the run branch's
#     tip when the run started, which the conductor exports), so it checks the whole spec diff,
#     operator commits that landed on the run branch directly included, never only the check-in
#     branch's own commits. An empty base refuses: the run recorded no base tip, so there is no
#     spec diff to check.
# A base that names no commit refuses too, rather than reading a failed diff as a clean one.

check=${1:-}
if [ $# -ge 2 ]; then
    base=$2
    test -n "$base" || {
        echo "$check gate: no run base - the run recorded no base tip, so there is no spec diff to check; refusing"
        exit 1
    }
else
    base=$(git rev-parse --verify -q rigger-run || git rev-parse --verify -q origin/main || git rev-parse --verify -q HEAD)
fi
git rev-parse -q --verify "$base^{commit}" > /dev/null || {
    echo "$check gate: base '$base' names no commit in this repository, so there is no diff to check; refusing"
    exit 1
}

# The lines the diff from the base adds under the pathspec "$@" (all of it when none), without
# the file headers.
added() {
    git diff "$base"...HEAD -- "$@" | grep "^+" | grep -v "^+++ "
}

# The lines the diff from the base removes under the pathspec "$@", without the file headers.
removed() {
    git diff "$base"...HEAD -- "$@" | grep "^-" | grep -v "^--- "
}

# Each event type declaration (a `const TYPE_<NAME>: &`) on the lines read from stdin, one per line
# as `TYPE_<NAME>`, a tab, and its value: the string literal it is declared as, quotes kept; for
# any other right-hand side, such as a re-export, the `TYPE_` name it reads; or nothing when the
# line holds neither.
declarations() {
    awk '{
        if (!match($0, /const TYPE_[A-Z0-9_]+: &/)) next
        name = substr($0, RSTART + 6, RLENGTH - 9)
        rest = substr($0, RSTART + RLENGTH)
        value = ""
        if (match(rest, /=[ \t]*"[^"]*"/)) {
            value = substr(rest, RSTART, RLENGTH)
            sub(/^=[ \t]*/, "", value)
        } else if (match(rest, /TYPE_[A-Z0-9_]+/)) {
            value = substr(rest, RSTART, RLENGTH)
        }
        print name "\t" value
    }'
}

# The lines read from stdin, sorted, unique and joined with ", ".
joined() {
    sort -u | awk '{ printf "%s%s", (NR > 1 ? ", " : ""), $0 }'
}

case "$check" in
style)
    em=$(printf "\342\200\224")
    if added | grep -qF "$em"; then
        echo "style gate FAILED: an em dash (U+2014) was introduced; use a hyphen"
        exit 1
    fi
    ;;
no-os-kill)
    # A shell-out to a termination utility, matched in any case: Windows, and macOS's default
    # filesystem, resolve a program name case-insensitively.
    shell_out="Command::new\(.(kill|pkill|killall|xkill|taskkill)"
    # The `--` argv separator, or a format! building a leading-hyphen (process-group) pid.
    group_target="\.arg\(.--.\)|format!\(.-\{"
    bad="[^a-zA-Z_]kill(all)? -[0-9A-Za-z]|[^a-zA-Z_]pkill[^a-zA-Z_]|killpg|libc::kill\(|signal::kill\(|kill_process\(|$group_target"
    outside=$(added src crates tests ":(exclude)crates/rigger-process/src/reap.rs" ":(exclude)tests/common/mod.rs")
    if printf '%s\n' "$outside" | grep -qiE "$shell_out" || printf '%s\n' "$outside" | grep -qE "$bad"; then
        echo "no-os-kill gate FAILED: an OS-level kill, a process-group target, or a direct signal call was introduced outside the sanctioned lifecycle helpers (crates/rigger-process/src/reap.rs, tests/common/mod.rs). Lifecycle is handle-bound: Child::kill()/wait(), or the shared helper. See specs/78-no-os-level-kills.md"
        exit 1
    fi
    sanctioned=$(added crates/rigger-process/src/reap.rs tests/common/mod.rs)
    if printf '%s\n' "$sanctioned" | grep -qiE "$shell_out" || printf '%s\n' "$sanctioned" | grep -qE "$group_target"; then
        echo "no-os-kill gate FAILED: a sanctioned lifecycle file shelled out to kill or formatted a process-group target; use the internal guarded signal call only"
        exit 1
    fi
    if added src crates tests ":(exclude)crates/rigger-process/src/reap.rs" | grep -qE "TerminateProcess\("; then
        echo "no-os-kill gate FAILED: the Windows process-termination call (TerminateProcess) was introduced outside crates/rigger-process/src/reap.rs, whose send_signal is its one sanctioned site. See specs/78-no-os-level-kills.md"
        exit 1
    fi
    ;;
no-new-event-type)
    # Each declaration the diff adds, judged against every one the base tree holds or the diff
    # removes: a literal value must be one already declared, and one already declared for its
    # name when the name is known; any other right-hand side must re-export a known type, or the
    # name itself must be known. A literal that passes makes its name known to a re-export.
    verdicts=$({
        {
            git grep --no-color -h -E "const TYPE_[A-Z0-9_]+: &" "$base" -- src crates
            removed src crates
        } | declarations | awk '{ print "known\t" $0 }'
        added src crates | declarations | awk '{ print "added\t" $0 }'
    } | awk -F '\t' '
        $1 == "known" {
            names[$2] = 1
            if ($3 ~ /^"/) { values[$3] = 1; valued[$2 "\t" $3] = 1; literal[$2] = 1 }
            next
        }
        { n++; name[n] = $2; value[n] = $3 }
        END {
            for (i = 1; i <= n; i++) {
                if (value[i] !~ /^"/) continue
                if (literal[name[i]] && !valued[name[i] "\t" value[i]]) print "revalued\t" name[i]
                else if (!values[value[i]]) print "new\t" name[i]
                else names[name[i]] = 1
            }
            for (i = 1; i <= n; i++) {
                if (value[i] ~ /^"/ || names[name[i]] || (value[i] != "" && names[value[i]])) continue
                print "new\t" name[i]
            }
        }')
    new=$(printf '%s\n' "$verdicts" | awk -F '\t' '$1 == "new" { print $2 }' | joined)
    revalued=$(printf '%s\n' "$verdicts" | awk -F '\t' '$1 == "revalued" { print $2 }' | joined)
    if [ -n "$new" ]; then
        echo "no-new-event-type gate FAILED: the diff declares a new event type ($new); carry the fact on an existing event type, as a status or a meta key"
    fi
    if [ -n "$revalued" ]; then
        echo "no-new-event-type gate FAILED: the diff changes the value of an existing event type ($revalued); every log already written names it by its value, so keep the value"
    fi
    if [ -n "$new$revalued" ]; then exit 1; fi
    ;;
*)
    echo "usage: sh .rigger/gates/content.sh <style|no-os-kill|no-new-event-type> [base]" >&2
    exit 2
    ;;
esac
