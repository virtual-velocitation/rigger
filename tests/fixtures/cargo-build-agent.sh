#!/bin/sh
# A fixture "agent" for spec 77 criterion 1 (ONE BUILD LOCATION): runs a REAL `cargo
# build` in its cwd - the worktree `Driver::spawn` sets via `current_dir` - so a test
# can prove a real cargo subprocess honors the CARGO_TARGET_DIR this driver injects
# from SpawnOpts.env, landing its build output there instead of embedding a `target/`
# dir in the worktree itself. Echoes the build's own pass/fail so a test can tell "no
# target/ dir because the build never really ran" apart from "no target/ dir because
# CARGO_TARGET_DIR redirected it" - the exact vacuous-pass this fixture rules out.
if cargo build --offline --quiet; then
  echo "CARGO_BUILD=ok"
else
  echo "CARGO_BUILD=failed"
fi
echo '{"id":"final","pass":true}'
