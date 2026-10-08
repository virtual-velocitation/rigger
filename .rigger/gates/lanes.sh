#!/bin/sh
# The feature lanes beyond the default one, run by CI (.github/workflows/rust.yml) and by the
# `lane-no-default` and `lane-core` gates (.rigger/workflow.yml) through this one script, so the
# loop and CI never run different commands. tests/ci_lanes.rs pins both callers and the derivation.
#
# Usage: sh .rigger/gates/lanes.sh <no-default|core>
#   LANES_DRY=1 prints each command (as `+ <command>`) and runs none.
#
# no-default: the light lane, without the default features - clippy over every workspace target
#   with warnings denied, then every workspace test.
# core: the pure lane (`--no-default-features --features core`). Its members are derived, never
#   listed: every workspace member whose manifest declares a `core` feature (cargo metadata). A
#   member that ships a binary compiles only its library under `core` - THE FEATURE SPLIT gates
#   the binary, and the tests that drive it, behind `store` - so it is linted on its library
#   alone; every other member is linted over all its targets and runs its library and
#   integration tests.
#
# A test step whose every test binary reports `running 0 tests` FAILS: a lane that ran no test
# proves nothing, so it never passes as if it had.

lanes_run() {
    echo "+ $*"
    test -n "${LANES_DRY:-}" || "$@" || exit
}

# Run the test command "$@", echoing its output, and fail when it fails or when none of its test
# binaries ran a test.
lanes_test() {
    echo "+ $*"
    test -z "${LANES_DRY:-}" || return 0
    { "$@" 2>&1; echo "lanes-exit $?"; } | awk -v step="$*" '
        BEGIN { code = -1 }
        /^lanes-exit [0-9]+$/ { code = $2; next }
        { print }
        /^running [0-9]+ tests?$/ && $2 > 0 { ran = 1 }
        END {
            if (code != 0) exit (code > 0 ? code : 1)
            if (!ran) { print "lanes: `" step "` ran no test: every test binary reported running 0 tests"; exit 1 }
        }' || exit
}

case "${1:-}" in
no-default)
    lanes_run cargo clippy --workspace --all-targets --no-default-features -- -D warnings
    lanes_test cargo test --workspace --no-default-features --no-fail-fast
    ;;
core)
    members=$(cargo metadata --no-deps --format-version 1 | python3 -c '
import json, sys
meta = json.load(sys.stdin)
workspace = set(meta["workspace_members"])
for package in meta["packages"]:
    if package["id"] in workspace and "core" in package["features"]:
        kinds = {kind for target in package["targets"] for kind in target["kind"]}
        print("lib" if "bin" in kinds else "all", package["name"])
') || { echo "lanes: cannot derive the core lane members from cargo metadata"; exit 1; }
    full=
    lib_only=
    while read -r scope name; do
        case "$scope" in
        all) full="$full -p $name" ;;
        lib) lib_only="$lib_only -p $name" ;;
        esac
    done <<EOF
$members
EOF
    test -n "$full" || { echo "lanes: no workspace member without a binary declares a core feature"; exit 1; }
    # $full and $lib_only are word lists of `-p <member>` pairs, split on purpose.
    lanes_run cargo clippy $full --all-targets --no-default-features --features core -- -D warnings
    test -z "$lib_only" ||
        lanes_run cargo clippy $lib_only --lib --no-default-features --features core -- -D warnings
    lanes_test cargo test $full --tests --no-default-features --features core --no-fail-fast
    ;;
*)
    echo "usage: sh .rigger/gates/lanes.sh <no-default|core>" >&2
    exit 2
    ;;
esac
