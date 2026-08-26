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
echo '{"id":"final","pass":true}'
