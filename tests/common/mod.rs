//! Shared test support for the integration suites - compiled once into `all` (a suite there
//! reaches it with `use crate::common;`) and into each suite with its own `[[test]]` entry that
//! declares `mod common;`, so each suite uses the subset it needs (hence the module-wide `dead_code`
//! allowance: an item used by one suite is genuinely unused in the next).
//!
//! Its first, still-largest concern is WHERE the product binary is ([`product_binary`]): a shared home because it
//! has sixteen readers and one correct answer, and because the obvious per-suite spelling is
//! wrong in a way no suite can see on its own (see [`product_binary_from`]). Each later
//! addition below earns its place here the same way - a helper two or more suites would
//! otherwise define independently and identically (spec 85's mandatory duplication sweep).

#![allow(dead_code)]

use rigger::keep_awake::KEEP_AWAKE_ENV;
use std::path::{Path, PathBuf};
use std::process::Command;

pub mod cli;
pub mod fixtures;
pub mod git;
pub mod lens;
pub mod mcp;
pub mod product_binary;
pub mod repo;
pub mod served;
#[cfg(unix)]
pub mod witness;
pub mod workflow_probe;

#[allow(unused_imports)]
pub use fixtures::wait_until;
#[allow(unused_imports)]
pub use product_binary::{product_binary_from, rigger_bin, suite_profile_dir};

/// A step run against a directory: a fixture seeding it, or a concurrent writer acting on it.
pub type DirStep = fn(&Path);

/// Asserts the reported `line` names every one of `parts` together (`why` naming the line).
pub fn assert_names(line: &str, parts: &[&str], why: &str) {
    assert!(parts.iter().all(|p| line.contains(p)), "{why}: {line}");
}

/// A finished shell's success and its output, stdout then stderr.
pub fn shell_outcome(out: &std::process::Output) -> (bool, String) {
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

/// How a session this suite launches through a host holds its process tree: the containment this
/// OS gives a tree the built binary keeps, as a composition root hands its host one
/// (`Containment::of_running_binary`), detected once per suite.
pub fn session_containment() -> &'static rigger::child_tree::Containment {
    static CONTAINMENT: std::sync::OnceLock<rigger::child_tree::Containment> =
        std::sync::OnceLock::new();
    CONTAINMENT.get_or_init(|| rigger::child_tree::Containment::detect(&rigger_bin()))
}

/// A `Command` for the compiled `rigger` binary this suite drives, defensively UNFENCED
/// (spec 70 criterion 3's gate store fence, ground (a) of the u3 reject:
/// adv-u3-fence-breaks-existing-store-precedence-tests-measured) - the ONE shared authority
/// every periphery/integration suite that spawns a store-opening courier
/// (`emit`/`result`/`peers`/`reported`/`prompt`/`progress`/`status`/`reset`) should build
/// its `Command` through, rather than `Command::new(rigger_bin())` directly.
///
/// WHY THIS EXISTS. `gate::ExecRunner::run` pins `RIGGER_STORE_FENCE_DIR` on the ENTIRE
/// subprocess tree of a unit-worktree gate (every unit's `test` gate is literally
/// `cargo test`, per `.rigger/workflow.yml`) so an INCIDENTAL courier that gate's own test
/// suite spawns can never walk up into the repo's live run stream. That env var is
/// inherited by every descendant process by default, including THIS test binary whenever
/// it itself runs as a fenced gate - so a periphery suite whose entire purpose is driving
/// the product binary through its OWN store-resolution/precedence logic against a fixture
/// of its own choosing (never the gate runner's intended protection target) would silently
/// observe the fenced scratch location instead, corrupting its own assertions. Clearing the
/// var here, in the one place every such suite already gets the binary's path from, means
/// every current and future courier-spawning call site is protected uniformly - no test
/// file has to remember this individually, matching spec 70's own stated principle that
/// "the fence is the gate runner's job, not each test's".
///
/// A command that writes the store (`graph build`, `canary`, `playbooks`, `setup`) reads it only
/// below its project root, where the fence takes the place of the root's store as it does a
/// courier's; at the root, and for every other command (`docs`, `init`, ...), clearing it is a
/// harmless no-op.
///
/// Also unconditionally strips an ambient `KURRENTDB_CONN` (spec 62 unit u62c4, round-6
/// adjudication `adj-u62c4-r6-verdict-reject-blast-radius-audit-incomplete`), mirroring the
/// `STORE_FENCE_ENV` strip above for the identical reason. `main.rs::store_selection_at`
/// gives an environment `KURRENTDB_CONN` (rung 2) precedence over a committed `store:`
/// config (rung 4+), and none of these throwaway test fixtures commit one - so a periphery
/// suite spawned through this helper on a machine whose `cargo test` process happens to
/// inherit a real, reachable, credentialed `KURRENTDB_CONN` (a documented, supported rigger
/// configuration this self-hosted project can itself run with) would otherwise silently
/// route that fixture's courier writes into the operator's real shared production event
/// store under a fake throwaway git project's identity, rather than resolving the local
/// sqlite store the fixture actually built. Applied here, in the one place every current
/// and future courier-spawning call site already gets the binary's path from, rather than
/// each test file having to remember it individually. A call site that deliberately wants
/// `KURRENTDB_CONN` propagated to the child (`store_precedence.rs`, `store_resolution_cli.rs`,
/// `store_secrets.rs`) is unaffected: it re-sets `.env("KURRENTDB_CONN", ...)` on the
/// returned `Command` after this call, and a later `.env()` call always wins over an earlier
/// `.env_remove()` for the same key.
///
/// Also pins `XDG_CACHE_HOME` to [`test_cache_home`] - ONE throwaway directory shared by
/// every subprocess the CURRENT TEST spawns (spec 89, criterion 2). The product's
/// scratch-root DEFAULT now lives under `$XDG_CACHE_HOME/rigger/<encoded repo>` rather than
/// inside each fixture's own `.rigger` (see [`rigger::worktree::scratch_root_path`]'s own
/// doc comment); leaving `XDG_CACHE_HOME` at its real ambient value here would make every
/// fixture repo's default-rooted scratch/worktrees land in the OPERATOR's real
/// `~/.cache/rigger/` - one orphaned, never-reclaimed directory per fixture tempdir this
/// suite has ever created, mirroring exactly why `run_rigger_envs` already defaults
/// `XDG_STATE_HOME` the same way. A call site that wants to test HOMELESS behavior
/// specifically (no `HOME`, no `XDG_CACHE_HOME`) still can: `.env_remove("XDG_CACHE_HOME")`
/// chained on the returned `Command` wins, exactly like a later `.env()` override does.
///
/// Also strips an ambient [`rigger::driver::SPAWN_ENV`]: a suite run from inside an agent
/// session inherits that session's spawn id, which would stamp every fixture `rigger emit` as
/// that spawn's and subject it to the emit core's id-ownership check. A test that drives the
/// stamp re-sets it with `.env(...)` on the returned `Command`, which wins.
///
/// Also strips an ambient [`rigger::gate_channel::GATE_SOCKET_ENV`]: a suite run from inside a
/// unit's spawn inherits the address of the live driver's gate socket, so every fixture
/// `rigger gate` would ask that driver instead of the fixture's own. A test that drives the
/// address re-sets it with `.env(...)`, which wins.
///
/// Also pins [`KEEP_AWAKE_ENV`] to a recording fake writing under [`test_cache_home`] (spec
/// 105, KEEP-AWAKE): a `rigger run` left on the OS adapter would hold the
/// operator's real machine awake, or print the keep-awake unavailable line on a host lacking the
/// facility, so no test outside the adapter's own rows depends on the host's power facility. A
/// test that drives the knob re-sets it with `.env(...)` or clears it with `.env_remove(...)`,
/// either of which wins.
///
/// Also pins [`RELAUNCH_ENV`] and [`LOGIN_START_ENV`] to recording fakes under
/// [`test_cache_home`], so no rigger a test runs starts a detached driver or registers a
/// relaunch with the host's login facility; a test that drives either knob re-sets it with
/// `.env(...)`, which wins.
pub fn rigger_courier() -> Command {
    let mut cmd = Command::new(rigger_bin());
    unfenced(&mut cmd);
    cmd
}

/// Applies [`rigger_courier`]'s own env hygiene (the `STORE_FENCE_ENV`/`KURRENTDB_CONN`/
/// `SPAWN_ENV`/`GATE_SOCKET_ENV` strip and the `XDG_CACHE_HOME`, keep-awake, relaunch and
/// login-start pins - see that function's doc
/// comment for the full WHY)
/// to an ALREADY-CONSTRUCTED `Command`, for the one class of call site `rigger_courier`
/// itself cannot cover: a suite that drives the product binary through an intermediate
/// shell (`sh -c "<installed hook command>"`) rather than invoking it directly. That
/// `Command::new("sh")` still runs the product binary as a descendant process, so it still
/// inherits an ambient `RIGGER_STORE_FENCE_DIR`/`KURRENTDB_CONN` exactly like a direct
/// `rigger_courier()` spawn would - `run_stopfailure_command_through_a_real_shell` in
/// `stop_failure_hook_periphery.rs` hit this precisely: under a fenced gate (`cargo test`
/// run via `gate::ExecRunner`, which pins the fence on its whole subprocess tree),
/// `require_store_dir` resolved the fenced scratch dir before ever reaching the real
/// courier project passed as `current_dir`, so the installed hook command exited 0 while
/// recording nothing - a false green outside a gate and a false red inside one, neither of
/// which said anything about the hook command itself.
/// One shared helper keeps that env hygiene from drifting between the two call shapes,
/// rather than a second hand-copied `env_remove` chain living beside `rigger_courier`'s.
///
/// A call site that only needs a SUBSET (`write_guard_hook_periphery.rs`'s own `sh -c`
/// round trip, which never opens a store at all) has no reason to call this - applying it
/// there would be a harmless no-op, but an unused one, so it does not.
pub fn unfenced(cmd: &mut Command) -> &mut Command {
    cmd.env_remove(rigger::gate::STORE_FENCE_ENV);
    cmd.env_remove("KURRENTDB_CONN");
    cmd.env_remove(rigger::driver::SPAWN_ENV);
    cmd.env_remove(rigger::gate_channel::GATE_SOCKET_ENV);
    cmd.env("XDG_CACHE_HOME", test_cache_home());
    let record = test_cache_home().join("keep-awake");
    cmd.env(KEEP_AWAKE_ENV, format!("record:{}", record.display()));
    cmd.env(
        RELAUNCH_ENV,
        format!("record:{}", relaunch_record().display()),
    );
    cmd.env(
        LOGIN_START_ENV,
        format!("record:{}", login_start_record().display()),
    );
    cmd
}

/// The variable that selects how a rigger invocation registers a run's relaunch at login: its
/// `record:<path>` value appends a `register <key> <program> <args...>` or `unregister <key>`
/// line to `<path>` in place of writing the host's login-start entry, and lists the entries
/// those lines leave standing in place of the host's.
pub use rigger::login_start::LOGIN_START_ENV;

/// The file the login-start registrations of this test's rigger invocations are recorded in
/// ([`unfenced`]), so no rigger a test runs registers with, or sweeps, the host's login facility.
pub fn login_start_record() -> PathBuf {
    test_cache_home().join("login-start")
}

/// The variable that selects how a rigger invocation relaunches a run whose driver died: its
/// `record:<path>` value appends a `relaunch <argv>` line to `<path>` in place of starting one.
pub use rigger::detached::RELAUNCH_ENV;

/// The file the relaunches of this test's rigger invocations are recorded in ([`unfenced`]), so
/// no rigger a test runs starts a detached driver unasked.
pub fn relaunch_record() -> PathBuf {
    test_cache_home().join("relaunch")
}

/// Clears this test process's own ambient `STORE_FENCE_ENV` (the fence a gate pins on its
/// whole subprocess tree) when created AND again when dropped, so a test that sets the fence
/// itself to simulate a fenced gate never leaks it into the next test on the same process.
pub struct StoreFenceCleared;

impl StoreFenceCleared {
    pub fn new() -> Self {
        std::env::remove_var(rigger::gate::STORE_FENCE_ENV);
        StoreFenceCleared
    }
}

impl Drop for StoreFenceCleared {
    fn drop(&mut self) {
        std::env::remove_var(rigger::gate::STORE_FENCE_ENV);
    }
}

/// One throwaway `XDG_CACHE_HOME` shared by every `rigger` subprocess the CURRENT TEST
/// spawns (spec 89, criterion 2) - PER TEST-OWNED THREAD, not a single directory for the
/// whole test binary, and that distinction is load-bearing, not cosmetic.
///
/// This used to be `static HOME: OnceLock<TempDir>` - one directory for the entire process.
/// That leaked every real byte a `rigger_courier()`-spawned subprocess ever wrote under it
/// (worktrees, agent scratch, the shared build cache), on every single test run, because
/// Rust NEVER drops a `'static`-storage-duration value - not at a normal return from `main`,
/// and not at `std::process::exit`, which the built-in libtest harness calls at the end of
/// every run regardless of pass/fail. No amount of `Drop` machinery hung off a `static` can
/// ever fire; that is a property of `static` itself, not a bug in `TempDir` (round 5 reject:
/// `adv-u89c2-test-cache-home-static-never-dropped-leaks-every-binary-run`, live-verified
/// there against a compiled test binary run directly, bypassing cargo).
///
/// The fix is `thread_local!` instead of `static`, which is NOT the same leak wearing a
/// different name: the built-in libtest harness spawns each `#[test]` fn on its OWN freshly
/// created OS thread and joins it before returning - independently confirmed with a
/// standalone probe binary before landing this, not assumed - so a `thread_local`'s `Drop`
/// genuinely DOES run, deterministically, the moment the test that used it finishes,
/// regardless of how the test binary's OWN process eventually exits. Every current call
/// site in this suite (`rigger_courier`, `default_scratch_root`) is invoked synchronously
/// from a test's own thread, never from a further-spawned worker thread (audited: no
/// `std::thread::spawn` closure anywhere under `tests/` calls either), so one cache home per
/// test-owning thread still gives every subprocess spawned WITHIN one test the same shared
/// root - the only thing that changes is WHEN it is reclaimed (at that test's own thread
/// exit, not "eventually, maybe, if something remembers to ask" - never, for a `static`).
/// Distinct fixture repos still resolve to distinct directories under it
/// ([`default_scratch_root`]/[`rigger::worktree::cache_scratch_root_from`]'s own injective
/// repo-path encoding), so no two tests' scratch state can collide by sharing this root -
/// true before this change and unaffected by it.
///
/// The cache home every rigger subprocess of the current test resolves.
pub fn test_cache_home() -> PathBuf {
    thread_local! {
        static HOME: std::cell::RefCell<Option<tempfile::TempDir>> =
            const { std::cell::RefCell::new(None) };
    }
    HOME.with(|cell| {
        cell.borrow_mut()
            .get_or_insert_with(|| {
                tempfile::tempdir().expect("create a throwaway XDG_CACHE_HOME for tests")
            })
            .path()
            .to_path_buf()
    })
}

/// The scratch root a `rigger` subprocess spawned through [`rigger_courier`] against `root`
/// (an otherwise-default fixture: no `defaults.workdir` configured, no `RIGGER_TMPDIR`
/// override) resolves BY DEFAULT (spec 89, criterion 2: SCRATCH IS OUTSIDE THE STORE TREE).
/// THE ONE derivation every suite that asserts on a default-rooted worktree/scratch path
/// shares, so a hand-duplicated formula can never drift from the real resolver the way the
/// old bare `root.join(".rigger").join("tmp")` literal used to (correctly, before this
/// criterion moved the default off that path entirely).
pub fn default_scratch_root(root: &Path) -> PathBuf {
    rigger::worktree::cache_scratch_root_from(
        root.to_str().expect("fixture root must be valid UTF-8"),
        Some(test_cache_home().into_os_string()),
        None,
    )
    .expect("a non-empty fixture root always resolves a cache-home scratch root")
}

/// The leaf `spawn_id` would own under `cache_home`'s former `rigger-mutants` root (spec 113
/// criterion 7 deleted that root): the one spelling of `<cache home>/rigger-mutants/<marker>`
/// every suite planting a leftover there shares.
pub fn cache_home_mutants_leaf(cache_home: &Path, spawn_id: &str) -> PathBuf {
    cache_home
        .join("rigger-mutants")
        .join(rigger::liveness::marker_filename(spawn_id).expect("a plantable spawn id"))
}

/// The `defaults.workdir` value that nests a fixture repo's scratch/worktree DEFAULT back
/// inside `repo`'s own unique tempdir (spec 89 criterion 2's governing operator ruling,
/// item 2: the in-process `conductor::run()` sweep) - the in-process counterpart to
/// [`rigger_courier`]'s `XDG_CACHE_HOME` pin for a SPAWNED subprocess.
///
/// WHY THIS EXISTS. [`rigger::worktree::scratch_root_path`]'s DEFAULT (third) precedence
/// rung reads the REAL process `XDG_CACHE_HOME`/`HOME` whenever a caller's `defaults.workdir`
/// (its SECOND rung) is empty - correct for the product, but a fixture that drives
/// `conductor::run()` directly (never spawning the `rigger` binary, so `rigger_courier`'s own
/// pin never applies) inherits that real ambient value too: every unit/review worktree such a
/// test creates lands under the OPERATOR's real `~/.cache/rigger/`, one orphaned directory per
/// fixture run, colliding with every other concurrently-running fixture and agent on the same
/// machine - the exact class of litter `rigger_courier` was fixed (round 6) to keep a spawned
/// courier out of. Handing this value to `cfg.workflow.defaults.workdir` wins the SECOND rung
/// outright, so the resolver never reaches the third rung at all.
///
/// A plain `String` threaded through the caller's own `Config`, deliberately NOT an
/// environment-variable pin: `cargo test` runs `#[test]` fns concurrently on separate
/// threads, so mutating this process's own `XDG_CACHE_HOME`/`HOME` here would race every
/// OTHER test reading it at the same moment (the exact hazard [`RestoreEnvVars`]'s own doc
/// comment already treats as load-bearing in this same file); a value carried on this one
/// call's own `Config` cannot race anything, since it never leaves that value.
///
/// Nested INSIDE `repo` on purpose, not a sibling of it: the directory disappears the moment
/// the caller's own fixture `TempDir` drops, with no separate reclaim step required - never a
/// second, independently-lived directory that could outlive `repo` and need its own guard.
pub fn isolated_workdir(repo: &Path) -> String {
    format!("{}/.rigger-test-scratch", repo.display())
}

/// Shared guard for every sanctioned test-side signal helper below (`terminate_pid`,
/// `stop_pid`, `interrupt_pid`) - panics for pid <= 1 (init, or "no real pid") or a pid equal to THIS test
/// process's own, either being a bug in the fixture handing over a pid to signal, never a
/// race worth tolerating silently the way an already-exited target is. `verb` names the
/// signal action in the panic message ("terminate", "SIGSTOP", ...) so each caller keeps its
/// own distinct, specific wording rather than a generic one.
///
/// Self-pid checked FIRST, deliberately: every test binary runs as pid 1 of its own namespace
/// under `.cargo/pidns-runner.sh` (spec 78, THE NAMESPACE RUNNER), so its own pid and the
/// literal 1 are the SAME number there - checking self first means that case always panics
/// with the more specific "own pid" message rather than the generic "not a real process" one,
/// deterministically, regardless of whether the namespace runner is in effect for a given
/// invocation.
///
/// Extracted (round 8, `adj-u62c3r7-verdict-reject-stop-pid-untested`'s bundled DRY finding)
/// out of `terminate_pid` and `stop_pid`, which previously duplicated this exact two-assert
/// guard verbatim save for the verb - now the one place either function's validation logic
/// lives, exercised by both functions' own periphery tests in
/// `tests/no_os_kill_test_helper_periphery.rs`.
fn validated_target_pid(pid: u32, verb: &str) {
    let self_pid = std::process::id();
    assert!(
        pid != self_pid,
        "refusing to {verb} pid {pid}: it is this test process's own pid"
    );
    assert!(pid > 1, "refusing to {verb} pid {pid}: not a real process");
}

/// Terminate a process this test does not hold a [`std::process::Child`] handle to (spec 78,
/// THE TEST HELPER), mirroring [`rigger::reap`]'s production counterpart `send_signal`. Every
/// test-side signal goes through the one call in [`delivered`], which this, [`stop_pid`] and
/// [`interrupt_pid`] share, in this file the `no-os-kill` gate licenses
/// (`tests/no_os_kill_audit.rs`). Every former `tests/cli.rs::reap_pid` and
/// `tests/reset_build_cache_periphery.rs` shelled-out SIGKILL call site is this function now.
///
/// Panics via [`validated_target_pid`] for pid <= 1 or this test process's own pid. Otherwise
/// SIGKILLs through the `rustix` syscall (never a shell-out, never `libc`, never a
/// process-group/negative pid) and ignores a delivery that fails because the target already
/// exited: that is exactly the state a "make sure this is dead" caller wants.
// Unix only: the signal goes through rustix's unix process calls.
#[cfg(unix)]
pub fn terminate_pid(pid: u32) {
    let _ = delivered(pid, "terminate", rustix::process::Signal::KILL);
}

/// SIGSTOP a process this test does not hold a [`std::process::Child`] handle to (spec 62,
/// criterion 3's own STOPPED-holder fixture: a job-control-stopped predecessor keeps its
/// listening socket bound but never `accept()`s, the exact scenario the HELD-PORT DIAGNOSIS
/// exists to explain) - added here, alongside [`terminate_pid`], because the `no-os-kill`
/// gate's exemption (`SANCTIONED_FILES`, `tests/no_os_kill_audit.rs`) is FILE-scoped, not a
/// fixed list of function names: this file is already one of the two files licensed to call
/// the signal API directly, so a second sanctioned function here - rather than each fixture's
/// own call site externally invoking an OS-level stop-signal utility - is the ONE place this
/// concern belongs, mirroring how [`terminate_pid`] already centralizes the SIGKILL case.
///
/// Same self-pid/pid<=1 guard as [`terminate_pid`], via the shared [`validated_target_pid`].
/// Unlike that best-effort void return, this returns whether the signal was actually
/// delivered: a caller with no `Child` handle to inspect (only a bare pid) has no other way to
/// notice a delivery failure - mirroring the success check a prior external-utility invocation
/// gave the two fixtures this replaces. Both the guard-panic paths and this false-return path
/// are pinned by dedicated tests in `tests/no_os_kill_test_helper_periphery.rs`.
// Unix only: the signal goes through rustix's unix process calls.
#[cfg(unix)]
pub fn stop_pid(pid: u32) -> bool {
    delivered(pid, "SIGSTOP", rustix::process::Signal::STOP)
}

/// SIGINT a process this test holds a [`std::process::Child`] handle to, as an operator's Ctrl-C
/// interrupts a driver in the foreground: std's handle ends a child only by SIGKILL, so the
/// interrupt goes through this file, beside [`stop_pid`], under the same guard and the same
/// delivered-or-not answer.
// Unix only: the signal goes through rustix's unix process calls.
#[cfg(unix)]
pub fn interrupt_pid(pid: u32) -> bool {
    delivered(pid, "SIGINT", rustix::process::Signal::INT)
}

/// Whether `signal` reached `pid`, under [`validated_target_pid`]'s guard with `verb` naming the
/// signal: the one test-side signal call, which [`terminate_pid`], [`stop_pid`] and
/// [`interrupt_pid`] share.
// Unix only: the signal goes through rustix's unix process calls.
#[cfg(unix)]
fn delivered(pid: u32, verb: &str, signal: rustix::process::Signal) -> bool {
    validated_target_pid(pid, verb);
    let Ok(raw) = i32::try_from(pid) else {
        return false;
    };
    let Some(rpid) = rustix::process::Pid::from_raw(raw) else {
        return false;
    };
    rustix::process::kill_process(rpid, signal).is_ok()
}

/// A process this test did not spawn and holds no [`std::process::Child`] handle to, owned from
/// the moment its pid is read: dropping the owner, on any path out of its scope and a panic's
/// unwind included, ends it through [`terminate_pid`], but only while the pid still carries the
/// start time it was read with ([`rigger::reap::still_started`], the witness check the production
/// reap runs before it signals). A pid that had already ended when it was read, or that was
/// recycled onto another process since, is never signalled.
// Unix only: the signal goes through rustix's unix process calls.
#[cfg(unix)]
pub struct OwnedPid {
    pid: u32,
    started: Option<u64>,
}

// Unix only: the signal goes through rustix's unix process calls.
#[cfg(unix)]
impl OwnedPid {
    /// Own `pid`, its start time read now.
    pub fn new(pid: u32) -> Self {
        Self::owning(pid, rigger::reap::pid_starttime(pid))
    }

    /// Own `pid` as read with the start time `started`, so a row can hand the owner a witness the
    /// pid no longer carries, as a recycled pid reads.
    pub fn with_witness(pid: u32, started: u64) -> Self {
        Self::owning(pid, Some(started))
    }

    /// Own `pid` as read with the start time `started`, `None` when it was not read.
    fn owning(pid: u32, started: Option<u64>) -> Self {
        Self { pid, started }
    }
}

// Unix only: the signal goes through rustix's unix process calls.
#[cfg(unix)]
impl Drop for OwnedPid {
    fn drop(&mut self) {
        if self
            .started
            .is_some_and(|started| rigger::reap::still_started(self.pid, started))
        {
            terminate_pid(self.pid);
        }
    }
}

/// Whether `pid` is still running: its run state, read through the product's process interface
/// ([`rigger::os_process::state`]), is readable and not `Z`, a zombie. An exited-but-unreaped
/// child is a zombie, which [`is_alive`]'s signal probe still reports alive.
pub fn is_running(pid: u32) -> bool {
    rigger::os_process::state(pid).is_some_and(|state| state != 'Z')
}

/// Whether `pid` is currently alive, via the internal `rustix` liveness probe (mirrors
/// [`terminate_pid`]'s signal call so both go through the identical sanctioned API) - the
/// test-side replacement for a shelled-out existence-probe command.
// Unix only: the signal goes through rustix's unix process calls.
#[cfg(unix)]
pub fn is_alive(pid: u32) -> bool {
    let Ok(raw) = i32::try_from(pid) else {
        return false;
    };
    let Some(rpid) = rustix::process::Pid::from_raw(raw) else {
        return false;
    };
    rustix::process::test_kill_process(rpid).is_ok()
}

/// Serializes the tests of one suite that read or write the process environment (or spawn a
/// subprocess, whose `Command` captures it): `cargo test` runs a binary's tests as concurrent
/// threads, and a concurrent env read racing a concurrent env write is a genuine POSIX
/// getenv/setenv hazard regardless of which keys either side touches. Hold the guard for the
/// whole test. A poisoned lock (a holder panicked) is recovered, never propagated.
pub fn env_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static ENV_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    ENV_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// RAII guard restoring a set of environment variables to their PRIOR value on drop -
/// captured before mutation, not unconditionally removed - so a test that redirects an
/// ambient var (`HOME`, `XDG_STATE_HOME`, `KURRENTDB_CONN`, ...) never permanently erases a
/// value this process had ambiently set for the rest of this test binary's life the moment
/// the guarded test ran. Shared by every suite that needs this (rather than each file
/// carrying its own bespoke Drop guard) so there is exactly ONE correct capture-and-restore
/// implementation on record, not a second, divergent copy that reintroduces the very
/// unconditional-`remove_var` bug this guard exists to fix (the defect class named in
/// `adv-u62c4-r5-uphold-remove-var-reintro-third-occurrence`).
pub struct RestoreEnvVars(Vec<(&'static str, Option<std::ffi::OsString>)>);

impl RestoreEnvVars {
    /// Captures each name's CURRENT value before the caller mutates it. Call this before any
    /// `std::env::set_var`/`remove_var` on the same names, never after.
    pub fn capture(names: &[&'static str]) -> Self {
        Self(names.iter().map(|&n| (n, std::env::var_os(n))).collect())
    }
}

impl Drop for RestoreEnvVars {
    fn drop(&mut self) {
        for (name, prior) in self.0.drain(..) {
            match prior {
                Some(v) => std::env::set_var(name, v),
                None => std::env::remove_var(name),
            }
        }
    }
}

/// A JSON object's own key set, sorted - the one comparison every wire-shape periphery test
/// (`console_map_frame_wire_shape_periphery.rs`, `console_map_explore_rail_wire_shape_periphery.rs`,
/// `console_map_legend_wire_shape_periphery.rs`, ...) needs to pin a serde type's field names
/// exactly, never more or fewer. Shared here rather than each file defining its own identical
/// copy (spec 85's own mandatory duplication sweep: a same-named helper independently defined in
/// 2+ files is a candidate regardless of the Jaccard pass).
pub fn json_object_keys(v: &serde_json::Value) -> Vec<String> {
    let mut keys: Vec<String> = v
        .as_object()
        .unwrap_or_else(|| panic!("expected a JSON object, got {v:?}"))
        .keys()
        .cloned()
        .collect();
    keys.sort_unstable();
    keys
}

/// A minimal spawn request: the deterministic id derived from `unit` + `role` + `attempt` (so
/// it cannot drift from the labels), every optional field empty.
pub fn spawn_request(
    unit: &str,
    stage: &str,
    role: &str,
    attempt: u32,
    prompt: &str,
) -> rigger::spawn::SpawnRequest {
    rigger::spawn::SpawnRequest {
        id: rigger::spawn::spawn_id(unit, role, attempt),
        unit: unit.to_string(),
        stage: stage.to_string(),
        prompt: prompt.to_string(),
        ..Default::default()
    }
}

/// The text after the last `::` of `path` (`a::b::T` -> `T`), the whole of it when it holds none:
/// the leaf of a definition's id path, a Rust type path or a module path.
pub fn last_path_segment(path: &str) -> &str {
    &path[path.rfind("::").map_or(0, |at| at + "::".len())..]
}
