#!/bin/sh
# A fixture "agent" for FAILURE CLASS (spec 104 criterion 5): the CONSTRAINTS WALK shape -
# "the child exits before `system/init` (binary missing, unknown flag) - a fault of class
# `unknown` carrying the stderr tail, never a hang." Reads and discards the host's first
# stdin message like every sibling fixture, writes one short diagnostic line to STDERR,
# then exits nonzero WITHOUT ever writing a single line to stdout - so the reader's stdout
# loop never runs at all and `read_stream` falls straight through to its no-result path
# with nothing but this stderr text to explain why.
IFS= read -r line
echo 'error: unrecognized flag --this-flag-does-not-exist' >&2
exit 1
