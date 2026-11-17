#!/bin/sh
# A stand-in for the Linux keeper (`crates/rigger-process/src/child_tree/linux.rs`), run as one in
# its place with the keeper's arguments: its status descriptor, its control descriptor, then the
# program it would spawn, which it never spawns. It writes the spawn record naming itself as the
# tree's main, unless its mode says otherwise, then misbehaves as `$RIGGER_TEST_KEEPER` names:
#
# `gone` exits at once, writing no other record, as a keeper ended from outside does.
# `lost` names `$RIGGER_TEST_MAIN` as the main instead, then exits at once, as a keeper ended from
# outside while its main runs on does.
# `deaf` starts a holder and stays until the test releases it (tests/fixtures/witness.sh), never
# reading its control descriptor, as a keeper that does not end its tree within its bound does.
#
# The status descriptor is written through /proc, since dash, a common /bin/sh, accepts no
# descriptor above 9 in a redirection.
#
# A checked-in executable fixture (never written at test time), so no concurrent test's fork+exec
# can inherit a write descriptor to it.
status=/proc/$$/fd/$1
case "$RIGGER_TEST_KEEPER" in
gone)
    printf 'spawned %s\n' "$$" >"$status"
    ;;
lost)
    printf 'spawned %s\n' "$RIGGER_TEST_MAIN" >"$status"
    ;;
deaf)
    self=$(readlink "$0") || self=$0
    . "$(dirname "$self")/witness.sh"
    printf 'spawned %s\n' "$$" >"$status"
    holder
    up
    held
    ;;
*)
    echo "fake-keeper: unknown mode: $RIGGER_TEST_KEEPER" >&2
    exit 2
    ;;
esac
