//! Shared test support for the integration suites - compiled into each suite that declares
//! `mod common;`, so each suite uses the subset it needs (hence the module-wide `dead_code`
//! allowance: an item used by one suite is genuinely unused in the next).
//!
//! Today it holds exactly one concern: WHERE the product binary is. That concern earns a shared
//! home because it has sixteen readers and one correct answer, and because the obvious per-suite
//! spelling is wrong in a way no suite can see on its own (see [`product_binary_from`]).

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// The product binary that belongs to the target dir a test executable is running out of, or
/// `None` when `test_exe` is not a cargo-run integration suite.
///
/// Cargo runs an integration suite from `<target>/<profile>/deps/<suite>-<hash>` and uplifts the
/// package's binaries one directory up, at `<target>/<profile>/`. Deriving the product from the
/// RUNNING executable therefore always names the target dir the test is actually in.
///
/// WHY THIS EXISTS RATHER THAN `CARGO_BIN_EXE_rigger` ALONE. That macro expands at COMPILE time
/// to an absolute path into the target dir the suite was COMPILED under. Cargo's fingerprints do
/// not include the target-dir path, so a target dir that has MOVED - copied, restored, or
/// relocated by a build-cache lifecycle - is still judged fresh: nothing recompiles, the baked
/// path no longer exists, and every suite that spawns the product dies with
/// `Os { code: 2, kind: NotFound }` before asserting anything. That is not hypothetical here: the
/// loop gives each unit's gates a per-unit `CARGO_TARGET_DIR`, and a relocated one turned a suite
/// that is green cold into 0 passed / 3 failed with exactly that error, while the recorded gate
/// evidence never named it. Runtime derivation cannot drift that way, because the answer is read
/// from the same directory tree the test itself was just loaded from.
///
/// It DECLINES (returns `None`) rather than guessing when the executable is not in a `deps/`
/// dir - a hand-run binary, a copied artifact - so [`rigger_bin`] can fall back honestly instead
/// of handing back a path nobody built.
pub fn product_binary_from(test_exe: &Path) -> Option<PathBuf> {
    let deps = test_exe.parent()?;
    if deps.file_name()? != "deps" {
        return None;
    }
    let profile = deps.parent()?;
    Some(profile.join(format!("rigger{}", std::env::consts::EXE_SUFFIX)))
}

/// The compiled `rigger` binary this suite drives - THE one authority every integration suite
/// calls, so the location rule is stated once and cannot drift per suite.
///
/// Resolution order, and why: [`product_binary_from`] over the running test executable first,
/// because it follows a moved target dir; the compile-time `CARGO_BIN_EXE_rigger` second, which
/// is the correct answer whenever nothing moved and covers a runner that stages test executables
/// outside `deps/`. This is the SINGLE site in `tests/` allowed to spell that macro, pinned by
/// `tests/product_binary_location.rs`. When neither path exists, it panics naming BOTH candidates
/// rather than letting the spawn fail with a bare `NotFound` that names nothing.
pub fn rigger_bin() -> PathBuf {
    let baked = PathBuf::from(env!("CARGO_BIN_EXE_rigger"));
    let derived = std::env::current_exe()
        .ok()
        .as_deref()
        .and_then(product_binary_from);
    match derived {
        Some(path) if path.exists() => path,
        derived => {
            if baked.exists() {
                return baked;
            }
            panic!(
                "no `rigger` binary to drive: derived {} / compiled-beside {}",
                derived
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "<none: the test executable is not in a deps/ dir>".into()),
                baked.display()
            )
        }
    }
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
/// This has no effect on a NON-courier command (`graph`, `docs`, `init`, ...): those never
/// read `RIGGER_STORE_FENCE_DIR` at all, so clearing it ahead of them is a harmless no-op.
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
pub fn rigger_courier() -> Command {
    let mut cmd = Command::new(rigger_bin());
    cmd.env_remove(rigger::gate::STORE_FENCE_ENV);
    cmd.env_remove("KURRENTDB_CONN");
    cmd
}

/// Terminate a process this test does not hold a [`std::process::Child`] handle to (spec 78,
/// THE TEST HELPER) - the ONE sanctioned test-side signal call, mirroring
/// [`rigger::reap`]'s production counterpart `send_signal` (the `no-os-kill` gate excludes
/// only these two functions from its tree-wide ban on shelling out to an OS termination
/// command or calling a signal API directly). Every former `tests/cli.rs::reap_pid` and
/// `tests/reset_build_cache_periphery.rs` shelled-out SIGKILL call site is this function now.
///
/// Panics for pid <= 1 (init, or "no real pid") or a pid equal to THIS test process's own -
/// either is a bug in the fixture handing it a pid to terminate, never a race worth
/// tolerating silently the way an already-exited target is. Otherwise SIGKILLs via the
/// internal `rustix` syscall (never a shell-out, never `libc`, never a process-group/negative
/// pid) and silently ignores ESRCH: the target having already exited is exactly the state a
/// "make sure this is dead" caller wants.
pub fn terminate_pid(pid: u32) {
    // Self-pid checked FIRST, deliberately: every test binary runs as pid 1 of its own
    // namespace under `.cargo/pidns-runner.sh` (spec 78, THE NAMESPACE RUNNER), so its own
    // pid and the literal 1 are the SAME number there - checking self first means that case
    // always panics with the more specific "own pid" message rather than the generic
    // "not a real process" one, deterministically, regardless of whether the namespace
    // runner is in effect for a given invocation.
    let self_pid = std::process::id();
    assert!(
        pid != self_pid,
        "refusing to terminate pid {pid}: it is this test process's own pid"
    );
    assert!(
        pid > 1,
        "refusing to terminate pid {pid}: not a real process"
    );
    let Ok(raw) = i32::try_from(pid) else {
        return;
    };
    let Some(rpid) = rustix::process::Pid::from_raw(raw) else {
        return;
    };
    let _ = rustix::process::kill_process(rpid, rustix::process::Signal::KILL);
}

/// Whether `pid` is currently alive, via the internal `rustix` liveness probe (mirrors
/// [`terminate_pid`]'s signal call so both go through the identical sanctioned API) - the
/// test-side replacement for a shelled-out existence-probe command.
pub fn is_alive(pid: u32) -> bool {
    let Ok(raw) = i32::try_from(pid) else {
        return false;
    };
    let Some(rpid) = rustix::process::Pid::from_raw(raw) else {
        return false;
    };
    rustix::process::test_kill_process(rpid).is_ok()
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
