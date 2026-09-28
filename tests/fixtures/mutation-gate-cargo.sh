#!/bin/sh
# A fixture standing in for `cargo` under the check-in mutation gate (.rigger/gates/mutation.sh):
# it appends one line per invocation - its argv, space-joined - to the file
# `$RIGGER_ARGV_CAPTURE` names, so a test reads back exactly how the gate launched each sweep
# without a real sweep ever running. A `cargo mutants` sweep writes an empty result where the
# real tool would (`mutants.out`, or `<--output>/mutants.out`) and exits 0; a `--list` prints
# nothing. Checked in and reached through a symlink, never written at test time, so no test
# ever executes a file another thread is still writing.
: "${RIGGER_ARGV_CAPTURE:?the test names the argv capture file}"
printf '%s ' "$@" >> "$RIGGER_ARGV_CAPTURE"
echo >> "$RIGGER_ARGV_CAPTURE"
out=.
list=0
while [ "$#" -gt 0 ]; do
    case "$1" in
        --output) out="$2"; shift ;;
        --list) list=1 ;;
    esac
    shift
done
[ "$list" = 1 ] && exit 0
mkdir -p "$out/mutants.out"
echo '{}' > "$out/mutants.out/outcomes.json"
: > "$out/mutants.out/missed.txt"
exit 0
