#!/bin/sh
# A fixture "agent" that echoes the build variables it was spawned with, so a test can assert
# what the agent-spawn injection site (Driver::spawn applying SpawnOpts.env) hands a real
# subprocess. Each reaches the agent only through SpawnOpts.env: a variable the workflow's
# `build.env` declares, resolved at the spawn's site (a `{build_cache}` one over the spawn's unit
# sibling, and left out for a dir with none), or a pair a test puts there itself. So a test can
# assert a declared variable reaches a real agent subprocess exactly as it reaches a real gate
# build, and that rigger sets no undeclared one.
echo "RUSTC_WRAPPER=$RUSTC_WRAPPER"
echo "SCCACHE_DIR=$SCCACHE_DIR"
echo "CARGO_INCREMENTAL=$CARGO_INCREMENTAL"
echo "CARGO_BUILD_JOBS=$CARGO_BUILD_JOBS"
echo "CARGO_TARGET_DIR=$CARGO_TARGET_DIR"
# The harness environment every headless worker gets (driver::harness_env), so a test can
# assert the cli host applies it, under the build environment, to a real subprocess.
echo "CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS=$CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS"
# The spawn id the host exports, so a test can assert the worker's own `rigger emit` is told
# the spawn it runs as.
echo "RIGGER_SPAWN=$RIGGER_SPAWN"
echo "CLAUDE_CODE_OAUTH_TOKEN=$CLAUDE_CODE_OAUTH_TOKEN"
echo "RIGGER_CLAUDE_CODE_OAUTH_TOKEN=$RIGGER_CLAUDE_CODE_OAUTH_TOKEN"
echo '{"id":"final","pass":true}'
# The task on stdin, read to its end: an author's task carries the handover contract, which the
# answer then ends with (`launch-args.sh`'s `hand_over`).
self=$(readlink "$0") || self=$0
. "$(dirname "$self")/launch-args.sh"
hand_over "$(cat)"
