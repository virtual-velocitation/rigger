#!/bin/sh
# A fixture standing in for the real `cargo` binary, for `build/console_wasm.rs`'s
# `build_wasm_artifact` periphery tests: records its own argv and every env var it
# actually received into $RECORDING_CARGO_DUMP_FILE (an env var the test sets before
# calling `build_wasm_artifact`), so a test can assert on the exact command line and the
# exact environment a REAL subprocess boundary sees - never a `Command` built in-process
# and inspected without ever being spawned. Exits with $RECORDING_CARGO_EXIT_CODE
# (default 0) so a test can drive both the success and failure return paths of
# `build_wasm_artifact` without ever running a real (slow) nested cross-compile.
{
  echo "ARGS:$*"
  env
} > "$RECORDING_CARGO_DUMP_FILE"
exit "${RECORDING_CARGO_EXIT_CODE:-0}"
