# shellcheck shell=sh
# Sourced, never run, by a gate that runs tests: points testcontainers at the operator's
# rootless podman, so a container-backed test RUNS under the gate instead of skipping, and
# removes the test containers an earlier run left behind.
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
#
# LEFTOVERS. A test removes its container when it drops it, but only inside its async runtime: a
# test binary that aborts, or a panic that drops the container outside the runtime, leaves the
# container in place, a running server holding its memory until someone removes it (two leaked
# KurrentDB servers measured 2026-09-29). So, once it has found the runtime, the snippet removes
# every container carrying the label `rigger.test` - the label rigger's test fixtures put on each
# container they start - that is older than RIGGER_TEST_CONTAINER_MAX_AGE_S seconds (600 by
# default, far past any test's own use of one), running or not, through the podman CLI, or the
# docker CLI when podman's is absent; with neither, one line says so and the gate goes on. The
# listing asks the runtime for that label alone, so a container without it is never listed and
# never removed. The age is now less the CreatedAt both CLIs print (`YYYY-MM-DD HH:MM:SS[.frac]
# +zzzz ZONE`), read with `date -d`; a container whose time does not read, or a max age that is
# not a number, is left in place, and a removal that fails (its test removed it first) is not
# the gate's failure.
podman_socket="${XDG_RUNTIME_DIR:-/run/user/$(id -u 2>/dev/null)}/podman/podman.sock"
if test -z "${DOCKER_HOST:-}" && test -S "$podman_socket"; then
    DOCKER_HOST="unix://$podman_socket"
    RIGGER_TEST_AS_BYTES="${RIGGER_TEST_AS_BYTES:-17179869184}"
    export DOCKER_HOST RIGGER_TEST_AS_BYTES
    container_cli=podman
    command -v podman >/dev/null 2>&1 || container_cli=docker
    if ! command -v "$container_cli" >/dev/null 2>&1; then
        echo "container-env: no podman or docker CLI on PATH, so no leftover test container is removed"
    else
        container_max_age="${RIGGER_TEST_CONTAINER_MAX_AGE_S:-600}"
        container_now="$(date +%s)"
        "$container_cli" ps -a --filter label=rigger.test --format '{{.ID}} {{.CreatedAt}}' |
            while read -r container_id container_day container_time container_zone _; do
                container_created="$(date -d "$container_day $container_time $container_zone" +%s 2>/dev/null)" ||
                    continue
                container_age=$((container_now - container_created))
                test "$container_age" -gt "$container_max_age" 2>/dev/null || continue
                if "$container_cli" rm -f "$container_id" >/dev/null 2>&1 </dev/null; then
                    echo "container-env: removed leftover test container $container_id (${container_age} s old)"
                fi
            done
    fi
fi
