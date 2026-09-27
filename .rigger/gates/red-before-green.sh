#!/bin/sh
# The red-before-green gate (TDD, workspace split step 0): on a unit branch, the first commit
# that touches source (`src/` or `crates/<name>/src/`) must be preceded by, or itself be, a
# commit that adds or changes a test. A test is a file under a `tests` directory or named
# `tests.rs`, or a hunk in a Rust source file that adds a `#[test]`/`#[cfg(test)]` item or
# lands inside the file's trailing `#[cfg(test)]` module.
#
# Usage: sh .rigger/gates/red-before-green.sh [run-branch]   (default: rigger-run, then
# origin/main). The unit's commits are those since its merge base with the run branch, oldest
# first, first-parent, merges skipped.

run_branch=${1:-rigger-run}
base=$(git merge-base HEAD "$run_branch" 2>/dev/null || git merge-base HEAD origin/main 2>/dev/null)
if [ -z "$base" ]; then
    echo "red-before-green: no merge base with $run_branch or origin/main; nothing to check"
    exit 0
fi

is_test_path() {
    case "$1" in
        tests/* | */tests/* | */tests.rs) return 0 ;;
    esac
    return 1
}

is_source_path() {
    case "$1" in
        src/* | crates/*/src/*) return 0 ;;
    esac
    return 1
}

# Whether commit $1 changes a test inside the Rust source file $2: an added test attribute, or
# a hunk that starts at or below the first column-0 `#[cfg(test)]` of the committed file.
has_test_hunk() {
    tests_from=$(git show "$1:$2" 2>/dev/null | awk '/^#\[cfg\(test\)\]/ { print NR; exit }')
    git show -U0 --format= "$1" -- "$2" | awk -v from="${tests_from:-0}" '
        /^\+[^+]/ && /#\[(test|cfg\(test\)|cfg\(all\(test)/ { found = 1 }
        /^@@/ {
            split($3, plus, ",")
            start = substr(plus[1], 2) + 0
            if (from > 0 && start >= from) found = 1
        }
        END { exit found ? 0 : 1 }'
}

seen_test=0
for commit in $(git rev-list --reverse --first-parent --no-merges "$base"..HEAD); do
    touches_source=0
    for file in $(git diff-tree --no-commit-id --name-only -r "$commit"); do
        if is_test_path "$file"; then
            seen_test=1
        elif is_source_path "$file"; then
            touches_source=1
            case "$file" in
                *.rs) has_test_hunk "$commit" "$file" && seen_test=1 ;;
            esac
        fi
    done
    if [ "$touches_source" = 1 ]; then
        if [ "$seen_test" = 1 ]; then
            exit 0
        fi
        echo "error[red-before-green]: $(git log -1 --format='%h %s' "$commit") is this unit's first source commit and no test commit precedes it - commit the failing test first (red), then the code that makes it pass (green); rebuild the branch with \`git reset --soft $(git rev-parse --short "$base")\` and two commits in that order"
        exit 1
    fi
done
exit 0
