#!/bin/sh
# The test gate: every workspace crate's tests. A bare `cargo test` tests the root package
# alone, so no crate's own unit tests would run under any unit; `--workspace` covers them all.
#
# The container runtime snippet (.rigger/gates/container-env.sh) is sourced first, in this
# shell, so what it exports - DOCKER_HOST at the local rootless podman when none is set -
# reaches the `cargo` started below and the container-backed tests run instead of skipping.
#
# The snippet is resolved from the working directory, the unit's worktree, whose tree may
# predate it: a missing snippet runs the tests without it, and a snippet that fails to source
# fails the gate before any test runs, never a silent run without the container runtime.
#
# Usage: sh .rigger/gates/test.sh   (from the worktree root)

if test -f .rigger/gates/container-env.sh; then
    . .rigger/gates/container-env.sh || exit 1
fi
cargo test --workspace
