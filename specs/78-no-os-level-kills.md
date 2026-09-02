# 78 - No OS-level kills: handle-bound process lifecycle, and a gate that keeps it so

**Goal:** rigger's own code signals processes by COMPUTED identity and shells out to kill(1).
Four sites exist: `src/budget.rs:265` (test `slot_releases_when_its_holder_process_exits_abnormally`
runs `kill -9 -- -<pgid>` - the only process-group kill in the tree), `src/reap.rs:153`
(`send_signal` shells out `kill -TERM`/`-KILL` to every pid whose `/proc` cwd lies under a
computed base dir), `tests/cli.rs:20476` (`reap_pid`: `kill -9 <pid read from a marker>`, 13
call sites) and `tests/reset_build_cache_periphery.rs:419,434` (`kill -0`/`kill -9` on a pid
read from a file). A computed target that resolves too wide is `kill(-1, SIGKILL)`: every
process the operator owns. Recorded: 2026-08-28 19:08:06, killsnoop captured `kill(-1,
SIGKILL)` from an exec'd `kill` during a u62c1 cargo-mutants sweep, preceded by one
`kill -9 -<pgid>` per test-suite run on an 80 s cadence; 2026-09-01 09:44:58, journal
`user@1000.service: Main process exited, code=killed, status=9/KILL` 69 s into a spec-62 step;
five earlier whole-session deaths on Aug 15-17 during spec-65 gate suites. The loop met this
exact shape on Aug 17 (DecisionMade `d65-u3-group-kill-argv-hazard`, store position 1786372:
"in a PID-namespaced test run this shape produced a literal kill(-1, SIGKILL)") and patched
the argv with `--` instead of removing the kill, then blessed negative-pid argv tree-wide.
Nothing mechanical forbade any of it. After this spec: zero OS-level kills in `src/` and
`tests/`; every process is ended through the `Child` handle that spawned it or through one of
two sanctioned internal helpers; a diff gate and a whole-tree audit test keep it that way.

## Design

SIGNAL API, decided here so no unit has to: `rustix` (`default-features = false`, features
`std`, `process`; the linux_raw backend, so the `--no-default-features` lane gains no `libc`
crate edge it does not already have - verify with `cargo tree -e normal --no-default-features
-i libc` before and after). Calls: `rustix::process::kill_process(Pid, Signal)` to signal,
`rustix::process::test_kill_process(Pid)` to probe liveness. No `nix`, no `libc`, no
`std::process::Command::new("kill")`/`pkill`/`killall`, no shell string containing a kill
command, anywhere in `src/` or `tests/`. `rustix::process::Pid::from_raw` accepts negative
values, so "pid > 1" is an explicit guard at both sanctioned sites, never an assumption.

SANCTIONED SITES, decided: exactly two functions may call the signal API. Production:
`src/reap.rs::send_signal`. Test fixtures: `tests/common/mod.rs::terminate_pid` (and its probe
`is_alive`). Everything else ends a process only through `std::process::Child::kill()` +
`wait()` on a child it spawned itself, or through the helper. The `no-os-kill` gate (landed in
`.rigger/workflow.yml`, wired into the implement stage) fails any unit whose ADDED lines in
`src/` or `tests/` outside those two files match: `Command::new(.(kill|pkill|killall|xkill)`,
a shell `kill -<x>`/`killall -<x>`, `pkill`, `killpg`, `libc::kill(`, `signal::kill(`,
`kill_process(`, `.arg(.--.)`, or `format!(.-{`; inside the two files a shell-out, `--`
separator or negative-pid format is still a failure. The gate matches comments too: describe
the old shell-out in prose ("the shell-out to kill(1)"), never paste the command text into a
code comment in `src/` or `tests/`. Prose in `docs/`, `specs/` and `.rigger/` is out of the
gate's scope and may name the rule literally.

THE REAPER (`src/reap.rs`), decided: identification stays cwd-based - rigger holds no handle to
the processes that root inside a worktree (an agent's `cargo`, `rustc`, test binaries, a dash
it started), which is the only reason a scan exists - but the kill step becomes safe by
construction:
- `is_reapable_base(base, authorized_root) -> Option<PathBuf>`: the base must canonicalize,
  exist, and lie STRICTLY under `authorized_root` (also canonicalized) - AMENDED round 2
  (decision `u78c2r2-authorized-root-caller-supplied`): `authorized_root` is a parameter the
  CALLER resolves and supplies, via the SAME authority it already used to build `base` itself
  (`rigger::worktree::scratch_root_path_from_env` for the run's own scratch tree; the
  registered mutation-scratch root under a cache home for the `cargo-mutants` tree, spec 77
  criteria 2-3) - never re-derived here from `base`'s own git/filesystem position. The
  original round-1 text pinned the boundary to a hardcoded `<repo>/.rigger/tmp` literal
  resolved from `base`'s own git context; that silently made every reap of a relocated
  scratch root (`defaults.workdir`/`RIGGER_TMPDIR`, a real, tested config surface -
  `tests/scratch_workdir_config.rs`) or of the registered mutation-scratch root (which by
  construction is never nested under any project's `.rigger/tmp`) an unconditional no-op -
  caught in review (adjudication reject, diff `6ca027f..622560c`) before landing. Refused -
  logged, `None` - for: an unresolvable `authorized_root`, `base` equal to it, a nonexistent
  `base` (fails to canonicalize), or a symlink that canonicalizes outside it
  ("reap refused: <base> is not strictly under <authorized_root>"). Never widens, never falls
  back, never signals on a refused base. [`Worktree::remove`] is the one exception (below):
  no `authorized_root` any caller could compute would reliably contain a worktree's own dir
  (the same relocation surface applies, with no necessary containment relationship to the
  repo at all), so it authorizes its reap by GIT IDENTITY instead and calls
  `reap::reap_authorized` directly, bypassing this containment gate entirely.
- Never signal pid <= 1, `std::process::id()`, or any ancestor of the current process (walk the
  `PPid:` chain in `/proc/<pid>/status`).
- Time-of-check/time-of-use: the scan records `(pid, starttime)` with starttime from
  `/proc/<pid>/stat` field 22; immediately before EACH signal the cwd and starttime are re-read
  and the pid is skipped if either differs (the pid was recycled by an unrelated process).
- Sequence unchanged: SIGTERM, grace, RE-SCAN, SIGKILL for whatever is still rooted inside -
  factored (round 2) into `reap::reap_authorized(base: PathBuf)`, `pub(crate)`, so
  `reap_processes_rooted_under` (`is_reapable_base` then this) and `Worktree::remove`'s own,
  independently (git-identity) authorized reap share the ONE termination implementation
  rather than a second, parallel one.
- Callers (round 2 signatures): `reap_then_remove_dir`/`reap_then_remove_worktree`
  (`src/main.rs`) and `reclaim_unit_mutation_scratch` (`src/driver/replay.rs`) now thread an
  `authorized_root` through to `reap_processes_rooted_under`, resolved from the SAME context
  each already had (the run's resolved scratch root, or the registered mutation-scratch root
  under `cache_home`); `Worktree::remove` (`src/worktree.rs`) calls `reap::reap_authorized`
  directly after its own `worktree_on_branch` check; the read-only users of
  `processes_rooted_under` are unaffected (that primitive's own single-argument signature is
  unchanged).
- `.cargo/mutants.toml` (landed, operator config - Notes below still name it not to be
  re-authored by any unit, round 2 included) excludes `reap::send_signal`,
  `reap::reap_processes_rooted_under` and `reap::is_reapable_base` from mutation: a mutant of
  a reaper is a loaded gun by definition; the guards are proven by explicit tests (criterion
  2), not by mutant survival. Round 2 found this exclusion has never actually taken mechanical
  effect for any function (`cargo-mutants` matches `exclude_re` against its mutant NAME
  string, `<file>:<line>:<col>: replace <fn> ...`, which never contains a `reap::`-qualified
  form) - a latent, pre-existing bug in this operator-owned file, left unfixed per the
  not-to-be-re-authored boundary and flagged for the operator instead (decision
  `u78c2r2-mutants-toml-exclude-re-latent-noop`). The loaded-gun rationale itself is
  unaffected: these functions are still proven by their own explicit tests, mechanically
  excluded or not, and `reap::reap_authorized` (round 2, the termination sequence factored out
  of `reap_processes_rooted_under`) carries the identical rationale.

THE BUDGET FIXTURE (`src/budget.rs` test), decided: the holder is ONE process that holds the
lock itself - `flock --no-fork -x <slot> sleep 300` (util-linux `-F` execs the command in place
of forking, so `sleep` owns the locked fd). Abnormal death is `holder.kill()` + `holder.wait()`
on the `Child` handle; no `process_group(0)`, no negative pid, no `--`. If the host's `flock`
lacks `--no-fork` the test skips with an explicit message rather than falling back to a
process-group kill (fallback stated).

THE TEST HELPER (`tests/common/mod.rs`), decided: `pub fn terminate_pid(pid: u32)` panics with
a message for pid <= 1 or pid == `std::process::id()`, otherwise SIGKILLs via rustix and
ignores ESRCH; `pub fn is_alive(pid: u32) -> bool` via `test_kill_process`. `tests/cli.rs::
reap_pid` is DELETED and its 13 call sites call `common::terminate_pid` (adding `mod common;`
to that file); `tests/reset_build_cache_periphery.rs` replaces its `kill -0` probe with
`is_alive` and its `kill -9` with `terminate_pid`. No other `#[cfg(test)]` module in `src/`
signals a non-child today; if one is found it uses `Child::kill()` or the helper, never a new
site.

THE NAMESPACE RUNNER (landed, operator config, containment not relaxation): `.cargo/config.toml`
sets `runner = ".cargo/pidns-runner.sh"`, which wraps every test binary in a user+pid+mount
namespace (uid preserved, no capabilities after exec, `/proc` shows only the namespace, all
descendants reaped when the binary exits; fails closed if the namespace cannot be created;
`RIGGER_PIDNS=off` only for hosted CI). Consequences test code must respect: a pid read from a
marker is a namespace pid; a detached process does not outlive its test binary; a test cannot
observe or signal anything outside its namespace. None of this relaxes the rule above - the
runner is the reason a regression or a mutant can no longer take the machine down while the
rule is enforced.

THE AUDIT TEST, decided: `tests/no_os_kill_audit.rs` walks every `.rs` file under `src/` and
`tests/` and fails naming file and line if any file other than `src/reap.rs` and
`tests/common/mod.rs` contains the gate's forbidden patterns, or if either sanctioned file
contains a shell-out, `--` separator or negative-pid format. It is the whole-tree twin of the
diff-scoped gate and runs in CI where the gate does not.

CONSTRAINTS WALK (results, so the panel does not rediscover them): empty (no process rooted
under the base) - the reap is a no-op. Repeated (reap called twice on one base) - idempotent;
ESRCH is ignored. Crash-resume (a rigger process dies mid-reap) - the reaper carries no state;
the next call re-scans, and the TOCTOU re-check covers pids recycled in between. Concurrent
actors (two rigger processes reap the same base) - both re-check cwd and starttime before each
signal, the second gets ESRCH; two different bases under `.rigger/tmp` are disjoint by cwd.
Cold start (fresh process, empty memory) - nothing to recover. Inside the namespace runner -
the scan sees only namespace pids, which is exactly the set the test may touch.

## Notes (non-criteria)

Landed before this run as operator config, referenced by the design and not to be re-authored
by any unit: `.cargo/pidns-runner.sh` + the `runner` entry in `.cargo/config.toml`;
`.cargo/mutants.toml`; the `no-os-kill` gate in `.rigger/workflow.yml`; `RIGGER_PIDNS: "off"`
in `.github/workflows/rust.yml`.

DOCUMENTED SCOPE BOUNDARY (decided in unit u78c4, discharging the disposition round 2 of
u78c2 left REQUIRED - decision `u78c2r2-verdict-approve-with-scoped-out-followup` - via the
class statement below rather than a site enumeration, per the binding operator scope decision
`d-u78c4-reap-coverage-scope-split-v2`): `src/reap.rs`'s module doc claims that before rigger
removes a dir it owns, it finds every process rooted inside and reaps it. That is an
unqualified reap COVERAGE claim, not a claim scoped to signalling form - read the same way by
this run's own `adj-u78c2r2-verdict-approve-with-scoped-out-followup`, which found it
contradicted by `sweep_terminal` and `clear_worktree_dir` (neither reaps before it removes);
this spec does not revisit or reinterpret that reading, only records it honestly as a known,
not-yet-corrected overclaim in that doc comment. This spec itself owns the FORM of process
signalling only: handle-bound termination, the two sanctioned sites, the diff gate, the audit
test. A removal path that does not reap a process rooted inside it before deleting its dir
keeps its pre-78 behavior unchanged by this spec either way - and since a removal path that
never reaps also never signals anything, it cannot violate this spec's rule no matter how many
such paths exist or where they live. Which removal paths reap before they remove, and which
don't yet - i.e. closing the gap between `src/reap.rs`'s doc comment and reality - is
`specs/79-reap-before-removal.md`'s scope: it owns the complete inventory (re-grounded at
implementation time, since site names, line numbers and call counts drift - the exact defect
that cost this unit three rounds of prose churn here) and the criteria that rewire each one
through `reap::reap_authorized` or a reap-then-remove helper.

Out of scope, deferred explicitly: a spawn LEDGER that would let the reaper signal only pids
rigger itself recorded (the scan stays, guarded); the dash's `--reap-on-idle` self-exit (it
exits itself, no signal involved); reap COVERAGE for any removal path not yet routed through
`reap::reap_authorized` or a reap-then-remove helper - owned by `specs/79-reap-before-removal.md`,
not enumerated here (per this spec's own class statement, none of them violates this spec's
signalling rule); anything else outside `src/` and `tests/`.

Why not keep `--`: `--` fixes one argv misparse; it does nothing about a pgid that IS 1, a
marker that names the wrong pid, or a base that canonicalizes too wide. The rule removes the
class, the gate removes the recurrence, the runner removes the blast radius.

## Global constraints

- Hyphens, never em dashes (U+2014 fails the style gate).
- Both feature lanes green: fmt, clippy `-D warnings`, test, on default and
  `--no-default-features`; the `no-os-kill` gate green on every unit's own diff.
- No new event type. No new dependency other than `rustix` as specified.
- After this spec no file under `src/` or `tests/` contains a shell-out to kill(1), a
  process-group (negative pid) target, or a direct signal call outside the two sanctioned
  functions - including in comments and test names.
- The operator's installed `rigger` binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves FIXTURES ARE HANDLE-BOUND: `slot_releases_when_its_holder_process_exits_abnormally`
  ends its single non-forking `flock --no-fork` holder through `Child::kill()` + `wait()` and
  still observes the kernel release the slot, and `tests/common/mod.rs::terminate_pid` refuses
  pid 0, pid 1 and the caller's own pid while terminating a child it is handed, with every
  former `reap_pid`, shell `kill -9` and `kill -0` site in `tests/cli.rs` and
  `tests/reset_build_cache_periphery.rs` now calling `common::terminate_pid`/`common::is_alive`.
  This criterion OWNS every test-side signal in `src/` `#[cfg(test)]` modules and `tests/`;
  the production reaper is criterion 2's, NOT this one's.
- [ ] a test proves THE REAPER CANNOT REACH OUTSIDE ITS BASE: `reap_processes_rooted_under`
  signals only through the internal rustix call, never pid <= 1, itself, or an ancestor, only
  after re-checking each target's cwd and start time immediately before the signal, and is a
  logged no-op for a base that is the repo root, `$HOME`, `/`, a nonexistent dir, `.rigger/tmp`
  itself, or a symlink under `.rigger/tmp` resolving outside it - while a SIGTERM-ignoring child
  rooted under a valid base is still reaped by the SIGKILL pass and a pid whose cwd changed
  between scan and signal is skipped. This criterion OWNS `src/reap.rs`; fixtures are
  criterion 1's, NOT this one's.
- [ ] a test proves THE TREE ASSERTS THE RULE: `tests/no_os_kill_audit.rs` scans every `.rs`
  file under `src/` and `tests/`, fails naming file and line on any forbidden pattern outside
  `src/reap.rs` and `tests/common/mod.rs` or on a shell-out inside them, and passes on the
  finished tree. This criterion OWNS the audit and introduces no signalling code of its own;
  it depends on criteria 1 and 2 having landed.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`), and
  the `no-os-kill` gate green on every unit's diff. This criterion OWNS this closing
  verification-only check; it introduces no code of its own and does not compete with
  criteria 1-3's ownership of their respective files.
