#!/bin/sh
# A stand-in `rigger` whose `hook stop-failure --spawn <id> --class <class>` finishes its write a
# second after it starts: a hook still writing when the session that ran it has exited. Its record
# lands under RIGGER_TEST_RECORDS as one file, the spawn's line then the class's, renamed into
# place whole, which the test imports into the progress store the host reads. It holds the
# witness's FIFOs (`witness.sh`, beside this file) while it runs, so the test reads whether it is
# still running. Any other command is refused.
self=$(readlink "$0") || self=$0
[ "$1 $2 $3 $5" = "hook stop-failure --spawn --class" ] || exit 2
. "$(dirname "$self")/witness.sh"
up
sleep 1
printf '%s\n%s\n' "$4" "$6" > "$RIGGER_TEST_RECORDS/.partial.$$"
mv "$RIGGER_TEST_RECORDS/.partial.$$" "$RIGGER_TEST_RECORDS/record.$$"
