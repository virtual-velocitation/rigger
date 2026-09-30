# shellcheck shell=sh
# Sourced, never run, by a gate that runs tests: points testcontainers at the operator's
# rootless podman, so a container-backed test RUNS under the gate instead of skipping.
#
# WHY. The KurrentDB event-store adapter's contract test
# (crates/rigger-store-sqlite/src/eventstore/kurrentdb.rs), like the root package's
# KurrentDB-backed tests, starts a real server through testcontainers and prints "skipping"
# and passes when it reaches no container runtime - right on a machine that has none.
# testcontainers looks for docker only (DOCKER_HOST, then docker's own socket paths), so on a
# workstation whose runtime is rootless podman it reaches nothing and skips: a gate passes the
# adapter untested, and the check-in mutation gate reports every mutant of it missed, a red no
# implementer can close because the one test that could catch them never runs.
#
# WHAT. Only when DOCKER_HOST is unset and the user's podman socket exists: DOCKER_HOST points
# at that socket, and the test runner's per-process address-space cap (RIGGER_TEST_AS_BYTES in
# .cargo/pidns-runner.sh, 4 GiB by default) rises to 16 GiB unless the caller already set one -
# under 4 GiB the contract fails to spawn its threads (EAGAIN, measured 2026-09-29). The cap is
# spelled in bytes: prlimit reads "16G" as 16 bytes, and no test binary would start at all. The
# raised cap holds for every test process the gate runs; the mutation sweep stays bounded as a
# whole by its own memory scope. testcontainers for Rust starts no reaper container, so nothing
# else needs setting: a test removes its container when it drops it. A DOCKER_HOST already set
# (a docker host the caller configured) leaves everything as it is.
podman_socket="${XDG_RUNTIME_DIR:-/run/user/$(id -u 2>/dev/null)}/podman/podman.sock"
if test -z "${DOCKER_HOST:-}" && test -S "$podman_socket"; then
    DOCKER_HOST="unix://$podman_socket"
    RIGGER_TEST_AS_BYTES="${RIGGER_TEST_AS_BYTES:-17179869184}"
    export DOCKER_HOST RIGGER_TEST_AS_BYTES
fi
