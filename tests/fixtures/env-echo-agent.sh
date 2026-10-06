#!/bin/sh
# A fixture "agent" for spec 65's ONE build-environment authority: it echoes the
# vars the resolver injects, so a test can assert the agent-spawn injection
# site (Driver::spawn applying SpawnOpts.env) reaches a real subprocess exactly
# like ExecRunner::run does for a gate build. CARGO_BUILD_JOBS (unit 4, JOBS CAP)
# is its own facet of the same resolver, independent of the wrapper vars above it.
# CARGO_TARGET_DIR (spec 77 criterion 1, ONE BUILD LOCATION) is a further
# independent facet - the per-unit cache `RunCtx::spawn_env` adds unconditionally,
# so a test can assert it reaches (or, for a dir with no per-unit cache, does NOT
# override) a real agent subprocess exactly like it reaches a real gate build.
echo "RUSTC_WRAPPER=$RUSTC_WRAPPER"
echo "SCCACHE_DIR=$SCCACHE_DIR"
echo "CARGO_INCREMENTAL=$CARGO_INCREMENTAL"
echo "CARGO_BUILD_JOBS=$CARGO_BUILD_JOBS"
echo "CARGO_TARGET_DIR=$CARGO_TARGET_DIR"
# The harness environment every headless worker gets (driver::harness_env), so a test can
# assert the cli host applies it, under the build environment, to a real subprocess.
echo "CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS=$CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS"
echo '{"id":"final","pass":true}'
