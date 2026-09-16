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
# Scratch placement (2026-09-11): every test binary gets a TMPDIR OUTSIDE the repository.
# A test fixture created under a TMPDIR nested inside the repo (agents pinned TMPDIR to
# .rigger/tmp/agent-scratch) sits inside the real git tree: a fixture with its own .rigger
# store but no .git resolves the REAL repo and the REAL scratch root while reading the
# fixture's unit-less events, and the step it drives sweeps every live unit worktree as
# terminal (u87c3, 2026-09-11 - all rigger-wt-* worktrees of a running spec vanished
# mid-round). Twelve store-walk unit tests fail the same way for the same nesting. The
# default lives on the large mount under the user cache, never under /tmp (the root
# partition) and never under any .rigger; RIGGER_TEST_TMPDIR overrides it explicitly.
# Applies to the RIGGER_PIDNS=off path too (CI), so the placement rule has one home.
TMPDIR="${RIGGER_TEST_TMPDIR:-${XDG_CACHE_HOME:-$HOME/.cache}/rigger/test-tmp}"
export TMPDIR
mkdir -p "$TMPDIR" 2>/dev/null || true
# The runner owns TMPDIR's lifecycle (2026-09-15). A test binary stopped mid-run (a gate or
# mutant timeout, a dead driver) never drops its tempdirs, and rigger's in-process unit tests
# resolve the cache-home scratch root from the ambient environment, so every fixture repo
# under TMPDIR used to leave one root behind in the OPERATOR's real ~/.cache/rigger - 105k of
# them (55 GB) in three days, until a shell glob over that directory exhausted memory. So:
# an entry idle for three hours is stale (nothing runs that long under the one-hour cap
# below) and is removed here; and the cache home for every test binary lives under TMPDIR,
# refreshed at each start so it is only ever swept when the whole suite has been idle that
# long. Inside it, rigger's own orphan-root reclaim keeps the directory small as tests run.
find "$TMPDIR" -mindepth 1 -maxdepth 1 -mmin +180 -exec rm -rf {} + 2>/dev/null || true
XDG_CACHE_HOME="$TMPDIR/xdg-cache"
export XDG_CACHE_HOME
mkdir -p "$XDG_CACHE_HOME" 2>/dev/null || true
touch "$XDG_CACHE_HOME" 2>/dev/null || true
# Test git never signs (2026-09-11): the operator's global git has commit.gpgsign=true and the
# ~43 `git commit` sites in the unit tests never turn it off, so every test commit runs gpg
# against the operator's keyring - keyring-lock contention and a one-in-ten `git worktree add`
# race whenever several agents run the suite at once (spec 90 criterion 1). Override ONLY the
# signing keys through git's environment-config channel (git >= 2.31), leaving the rest of
# the operator's config (init.defaultBranch, aliases) untouched; production `rigger` runs
# are not under this runner and still sign with the operator's own config.
GIT_CONFIG_COUNT=2
GIT_CONFIG_KEY_0=commit.gpgsign
GIT_CONFIG_VALUE_0=false
GIT_CONFIG_KEY_1=tag.gpgsign
GIT_CONFIG_VALUE_1=false
# Test git never signs OR prompts, and always commits as the same fixed identity
# (2026-09-15, spec 90 criterion 1): the block above closed the gpg half; the ~43 `git commit`
# sites still relied on ambient identity (repo config, or nothing - which fails outright with
# no operator gitconfig at all) and nothing stopped an interactive credential/passphrase
# prompt from hanging a test binary that has no TTY to answer it. Fixed author/committer
# identity, unrelated to and un-asserted by any existing test, so every commit succeeds
# deterministically; GIT_TERMINAL_PROMPT=0 refuses any prompt instead of hanging on one. A
# test that wants ITS OWN identity still gets it - these are plain environment variables, so a
# test's own `.env("GIT_AUTHOR_NAME", ...)` on its own `Command` overrides this default for
# that one child process, same as it always could.
GIT_AUTHOR_NAME=rigger-test
GIT_AUTHOR_EMAIL=rigger-test@localhost
GIT_COMMITTER_NAME=rigger-test
GIT_COMMITTER_EMAIL=rigger-test@localhost
GIT_TERMINAL_PROMPT=0
export GIT_CONFIG_COUNT GIT_CONFIG_KEY_0 GIT_CONFIG_VALUE_0 GIT_CONFIG_KEY_1 GIT_CONFIG_VALUE_1 \
  GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL GIT_TERMINAL_PROMPT
# Low CPU priority for every test binary too (2026-09-11): a run's review fan-out runs many
# full suites at once; tests yield to the operator's interactive work like compiles do.
#
# The test process dies with its launcher (2026-09-12): cargo-mutants times a mutant out by
# ending its `cargo test` child, and cargo is the runner's parent - nothing told the runner.
# The namespace and the test binary under it survived every timeout, kept cargo-mutants'
# output pipe open (the sweep sat on one mutant for an hour, reading a pipe held by an
# orphan), and each orphan spun at 7-20 cores under systemd --user, one for 6.6 hours.
# `setpriv --pdeathsig=KILL` (util-linux >= 2.33) marks THIS pid (nice, setpriv and unshare
# all exec in place, so it is the same pid cargo waits on) to receive SIGKILL the moment its
# parent exits; `--kill-child` then takes the test binary with it. The plain path gets the
# same mark so a CI container cannot hang a sweep either.
if [ "${RIGGER_PIDNS:-on}" = "off" ]; then
  # TERM here, not KILL: `timeout` forwards a TERM it receives to the test binary and exits;
  # a KILL would end only `timeout` and orphan the binary. The cap below still bounds a
  # binary that ignores TERM.
  exec nice -n 10 setpriv --pdeathsig=TERM timeout -s KILL "${RIGGER_TEST_MAX_SECS:-3600}" "$@"
fi
if [ -n "${RIGGER_PIDNS_TRACE:-}" ]; then
  echo "pidns-runner: $1" >&2
fi
if ! unshare --user --map-current-user --pid --fork --mount-proc true 2>/dev/null; then
  echo "pidns-runner: cannot create a user+pid namespace on this host; REFUSING to run the test binary unsandboxed: $1" >&2
  echo "pidns-runner: set RIGGER_PIDNS=off only in a throwaway environment (never on a workstation)." >&2
  exit 1
fi
# Memory bound (2026-09-02): cap each test binary's address space (default 24G,
# RIGGER_TEST_AS_BYTES to tune). A cargo-mutants mutant of a loop allocated ~32G on
# 2026-09-02 02:18; the kernel OOM killer took the test, and systemd then stopped the
# whole terminal scope as oom-kill collateral, ending the operator's session. With the
# cap, a runaway mutant fails its allocation and the test - the box never feels it.
# Lifetime bound (2026-09-12): inside the namespace, `timeout` (pid 1 in there) owns the test
# binary and ends it after RIGGER_TEST_MAX_SECS (default one hour - no single test binary of
# this crate runs that long even under a load of 200). A mutant that turns a loop infinite,
# or a binary whose launcher is already gone, can burn cores for at most that long instead of
# for days (one did: eight days at 17 cores, 2026-09-03 .. 09-11). Exit status 137 = capped.
exec nice -n 10 setpriv --pdeathsig=KILL unshare --user --map-current-user --pid --fork --mount-proc --kill-child -- timeout -s KILL "${RIGGER_TEST_MAX_SECS:-3600}" prlimit --as="${RIGGER_TEST_AS_BYTES:-25769803776}" -- "$@"
