#!/bin/sh
# Cargo test runner: every test binary runs inside a fresh user+pid+mount namespace.
#
# WHY (load-bearing - read before removing): rigger's own tests and reapers signal processes
# by COMPUTED identity - a process-group id, a pid read from a marker, a /proc cwd scan. A
# computed target that resolves too wide becomes kill(-1, SIGKILL): every process the operator
# owns. That destroyed the operator's entire desktop session (Xorg, gnome-shell, the terminal,
# systemd --user) at least seven times between 2026-08-15 and 2026-09-01, twice from a cargo
# test suite under cargo-mutants. Inside this namespace the ONLY reachable processes are the
# test binary's own descendants: kill(-1) kills the namespace and nothing else, and a /proc
# scan sees nothing but the namespace. The kernel also reaps every descendant when the test
# binary (pid 1 in here) exits, so no test can leak a detached process into the operator's
# session. The test still runs as the real uid (--map-current-user) with NO capabilities after
# exec, so file-permission and ownership tests behave exactly as outside.
#
# This is containment, not the fix: the standing rule is that rigger issues NO OS-level kills
# at all - lifecycle is handle-bound and internal (see specs/78-no-os-level-kills.md and the
# `no-os-kill` gate in .rigger/workflow.yml). The runner exists so a regression, a mutant, or a
# not-yet-fixed tree can never again take the machine down while that rule is being enforced.
#
# Fails CLOSED: if the namespace cannot be created, the test binary is NOT run unsandboxed.
# RIGGER_PIDNS=off        opt out - ONLY for a throwaway environment (a CI container without
#                         unprivileged user namespaces); never on an operator's workstation.
# RIGGER_PIDNS_TRACE=1    print one line per invocation to stderr (proof that the runner ran).
set -u
if [ "${RIGGER_PIDNS:-on}" = "off" ]; then
  exec "$@"
fi
if [ -n "${RIGGER_PIDNS_TRACE:-}" ]; then
  echo "pidns-runner: $1" >&2
fi
if ! unshare --user --map-current-user --pid --fork --mount-proc true 2>/dev/null; then
  echo "pidns-runner: cannot create a user+pid namespace on this host; REFUSING to run the test binary unsandboxed: $1" >&2
  echo "pidns-runner: set RIGGER_PIDNS=off only in a throwaway environment (never on a workstation)." >&2
  exit 1
fi
exec unshare --user --map-current-user --pid --fork --mount-proc --kill-child -- "$@"
