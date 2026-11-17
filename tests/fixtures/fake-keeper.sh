#!/bin/sh
# A stand-in for the Linux keeper (`crates/rigger-process/src/child_tree/linux.rs`), run as one in
# its place with the keeper's arguments: its status descriptor, its control descriptor, then the
# program it would spawn, which it never spawns. It writes the spawn record naming itself as the
# tree's main, then misbehaves as `$RIGGER_TEST_KEEPER` names:
#
# `gone` exits at once, writing no other record, as a keeper ended from outside does.
# `deaf` starts a holder and stays until the test releases it (tests/fixtures/witness.sh), never
# reading its control descriptor, as a keeper that does not end its tree within its bound does.
#
# A checked-in executable fixture (never written at test time), so no concurrent test's fork+exec
# can inherit a write descriptor to it.
status=$1
case "$RIGGER_TEST_KEEPER" in
gone)
    printf 'spawned %s\n' "$$" >&"$status"
    ;;
deaf)
    self=$(readlink "$0") || self=$0
    . "$(dirname "$self")/witness.sh"
    printf 'spawned %s\n' "$$" >&"$status"
    holder
    up
    held
    ;;
*)
    echo "fake-keeper: unknown mode: $RIGGER_TEST_KEEPER" >&2
    exit 2
    ;;
esac
