#!/bin/sh
# The content gates: checks over the lines a diff ADDS, each failing its gate when an added line
# carries a forbidden shape, and passing on a clean diff.
#
#   style (Gap 4a): a mechanically-checkable style constraint as a gate rather than ambient
#     reviewer attention - no em dash (U+2014) in any added text: code, comments, docs, prompts.
#     The byte pattern is generated at runtime via `printf` octal (portable to POSIX sh / dash and
#     bash), so this script carries no literal em dash and passes on its own diff.
#   no-os-kill (spec 78): rigger issues NO OS-level kills. Process lifecycle is handle-bound
#     (`Child::kill()`/`wait()` on a child this process spawned, or a job object it created) or,
#     in exactly two sanctioned places, an internal guarded signal call: the worktree reaper
#     (crates/rigger-process/src/reap.rs) and the shared test lifecycle helper
#     (tests/common/mod.rs). Everywhere else in src/, crates/ and tests/, an added line that shells
#     out to kill/pkill/killall/xkill/taskkill (the name in any case), formats a negative
#     (process-group) pid, uses the `--` argv separator, or calls a signal API directly FAILS.
#     Inside the two sanctioned files a shell-out is still a failure. A process-group signal call
#     (kill_process_group, kill_current_process_group, killpg) FAILS in every file, the two
#     sanctioned ones included: a group can hold processes rigger never started. Some calls only
#     the reaper may make: the Windows termination calls, TerminateProcess in send_signal and
#     TerminateJobObject in end_job, which ends a job object this process created; and the signal
#     calls by another route than the sanctioned one, tgkill, sigqueue and pidfd_send_signal. An
#     added one anywhere else, the test helper included, FAILS. Scoped to code: prose in docs/,
#     specs/ and .rigger/ may name the rule.
#     WHY: computed kill targets (a pgid, a marker pid, a /proc scan) resolved to kill(-1) and
#     destroyed the operator's whole desktop session seven-plus times; the review panel approved
#     each site because nothing mechanical forbade it. Now something does.
#
# Usage: sh .rigger/gates/content.sh <style|no-os-kill> [base]
# Which diff is checked is the caller's stage, and both forms take a three-dot diff to HEAD:
#   no base - a unit's own changes: from the run branch (`rigger-run`, with an origin/main / HEAD
#     fallback), run in the unit worktree, so it reports only THIS branch's changes, never the
#     whole tree.
#   base - everything since `base`: the check-in stage passes "$RIGGER_RUN_BASE" (the run branch's
#     tip when the run started, which the conductor exports), so it checks the whole spec diff,
#     operator commits that landed on the run branch directly included, never only the check-in
#     branch's own commits. An empty base refuses: the run recorded no base tip, so there is no
#     spec diff to check.
# A base that names no commit, or one sharing no history with HEAD, refuses too, rather than
# reading a failed diff as a clean one.

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
git merge-base "$base" HEAD > /dev/null || {
    echo "$check gate: base '$base' shares no history with HEAD, so there is no diff to check; refusing"
    exit 1
}

# The lines the diff from the base adds under the pathspec "$@" (all of it when none), without
# the file headers.
added() {
    git diff "$base"...HEAD -- "$@" | grep "^+" | grep -v "^+++ "
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
    # A process-group signal call, in any file.
    group_signal="kill_(current_)?process_group|killpg"
    if added src crates tests | grep -qE "$group_signal"; then
        echo "no-os-kill gate FAILED: a process-group signal call (kill_process_group, kill_current_process_group or killpg) was introduced; no file may signal a group, crates/rigger-process/src/reap.rs and tests/common/mod.rs included. See specs/78-no-os-level-kills.md"
        exit 1
    fi
    # A shell-out to a termination utility, matched in any case: Windows, and macOS's default
    # filesystem, resolve a program name case-insensitively.
    shell_out="Command::new\(.(kill|pkill|killall|xkill|taskkill)"
    # The `--` argv separator, or a format! building a leading-hyphen (process-group) pid.
    group_target="\.arg\(.--.\)|format!\(.-\{"
    bad="[^a-zA-Z_]kill(all)? -[0-9A-Za-z]|[^a-zA-Z_]pkill[^a-zA-Z_]|libc::kill\(|signal::kill\(|kill_process\(|$group_target"
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
    # The calls only the reaper may make.
    reaper_only="Terminate(Process|JobObject)\(|tgkill|sigqueue|pidfd_send_signal"
    if added src crates tests ":(exclude)crates/rigger-process/src/reap.rs" | grep -qE "$reaper_only"; then
        echo "no-os-kill gate FAILED: a call only the reaper may make (TerminateProcess, TerminateJobObject, tgkill, sigqueue or pidfd_send_signal) was introduced outside crates/rigger-process/src/reap.rs, the one file that may make them. See specs/78-no-os-level-kills.md"
        exit 1
    fi
    ;;
*)
    echo "usage: sh .rigger/gates/content.sh <style|no-os-kill> [base]" >&2
    exit 2
    ;;
esac
