#!/bin/sh
# A fixture "agent" for spec 105's hold transitions: prints the stream lines of the file
# `RIGGER_TEST_SCRIPTED_STREAM` names, so a test states a session's whole wire from its own table.
# It ends as the real host does: a session that wrote a `result` exits only when the host closes
# its input, and one that did not exits at once, as a session cut off with no result does. A
# checked-in executable fixture, for the same reason as every sibling fixture here: no concurrent
# test's fork+exec can inherit a write fd to it.
self=$(readlink "$0") || self=$0
. "$(dirname "$self")/launch-args.sh"
sid=$(launched_session "" "$@")
IFS= read -r _first
as_launched "$sid" <"$RIGGER_TEST_SCRIPTED_STREAM"
case $(cat "$RIGGER_TEST_SCRIPTED_STREAM") in
    *'"type":"result"'*)
        while IFS= read -r _line; do :; done
        ;;
esac
