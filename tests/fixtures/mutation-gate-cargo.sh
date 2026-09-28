#!/bin/sh
# A fixture standing in for `cargo` under the check-in mutation gate (.rigger/gates/mutation.sh):
# it appends one line per invocation - its argv, space-joined - to the file
# `$RIGGER_ARGV_CAPTURE` names, so a test reads back exactly how the gate launched each sweep
# without a real sweep ever running. A `cargo mutants` sweep writes an empty result where the
# real tool would (`mutants.out`, or `<--output>/mutants.out`) and exits 0; a `--list` prints
# nothing. Checked in and reached through a symlink, never written at test time, so no test
# ever executes a file another thread is still writing.
#
# `$RIGGER_FIXTURE_ENDED` makes the sweep also write one mutant log whose phase ended the way
# a process ended by the kernel's OOM reaper reads in cargo-mutants 27.1's log (the shapes a
# real sweep wrote on 2026-09-28): `test` - the test phase's own process ended by signal 9;
# `build` - the build phase's own process ended; `rustc` - a compiler process under the build
# ended, which cargo-mutants files as an unviable mutant.
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
name='crates/alpha/src/lib.rs:2:5: replace f -> u8 with 0'
build='*** /cargo nextest run --no-run --verbose --package=alpha@0.1.0'
test='*** /cargo nextest run --verbose --package=alpha@0.1.0 --package=fixture-root@0.1.0'
case "${RIGGER_FIXTURE_ENDED:-}" in
    test) body="$build\n*** result: Success\n$test\n*** result: Signalled(9)" ;;
    build) body="$build\n*** result: Signalled(9)" ;;
    rustc) body="$build\n  process didn't exit successfully: \`rustc --crate-name alpha\` (signal: 9)\n*** result: Failure(101)" ;;
    *) exit 0 ;;
esac
mkdir -p "$out/mutants.out/log"
printf '*** %s\n*** mutation diff:\n%b\n' "$name" "$body" > "$out/mutants.out/log/crates__alpha__src__lib.rs_line_2_col_5.log"
exit 0
