//! The supervised child-process guard (workspace split: moved from the dash, whose auto-started
//! child it first wrapped, beside the agent host that reaps through it too). The root `rigger`
//! crate re-exports it under its historical `rigger::dash::ReapedChild` path.

/// A supervised handle over a long-lived `rigger` child PROCESS - the auto-started
/// dashboard, and any future `rigger` child a run spawns. When this guard is dropped,
/// the child is KILLED and REAPED, so it can never outlive the run that started it.
///
/// This is the single reaping mechanism the dash and the other `rigger` children rely
/// on (spec 19b, unit 3: no orphaned `rigger` processes). `Drop` runs on BOTH a normal
/// scope exit AND an unwinding panic, so a normally-finishing OR a crashing driver
/// leaves no orphaned `rigger` process reparented to `init`. Reaping is `kill` followed
/// by `wait` (not `kill` alone): the `wait` collects the exited child, so a
/// finished-but-unwaited process leaves no defunct zombie either.
///
/// It is deliberately `std`-only (`std::process::Child`, not a `PR_SET_PDEATHSIG`
/// `prctl`): `libc` is an optional feature-gated dependency, but this guard must compile
/// on BOTH the default and the `--no-default-features` lane, and `std::process` is the
/// only child-lifecycle primitive available on both.
///
/// The other long-lived child is supervised by the same DISCIPLINE at its own ownership
/// boundary, not through this handle:
///   - `rigger serve` is spawned ONLY by the Node shim over an stdio transport, so the
///     Rust conductor never holds its `Child` to wrap in a Rust guard. Its
///     kill-on-parent-exit is STRUCTURAL: [`crate::mcpserver::Server::run`] serves only
///     until the input closes (the shim's stdin), and the OS closes that pipe whenever
///     the shim dies - a clean exit, a thrown error, or an uncatchable signal alike - so
///     an orphaned `rigger serve` sees EOF on stdin and exits on its own.
pub struct ReapedChild {
    child: std::process::Child,
}

impl ReapedChild {
    /// Take ownership of an already-spawned child so it is reaped when this guard drops.
    /// The caller owns spawning (dependency injection); this guard owns only its death.
    pub fn new(child: std::process::Child) -> Self {
        ReapedChild { child }
    }

    /// The supervised child's OS process id (e.g. to log the serving dash, or surface it
    /// in `rigger status`).
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// Borrow the supervised child mutably - a caller that must still interact with the
    /// live process (take its stdio pipes, drop its stdin to signal EOF) while this guard
    /// keeps owning the eventual reap. `pub(crate)`: an in-crate escape hatch for a
    /// caller that needs more than `id()`, never a public API a caller outside this
    /// crate should reach for (spec 104 round-4 REQUIRED FIX 2 - `driver::claude_code`'s
    /// `read_stream` is the first such caller, taking ownership of `Launch::child` and
    /// wrapping it here instead of re-deriving this same Drop-based reap guard itself).
    pub(crate) fn child_mut(&mut self) -> &mut std::process::Child {
        &mut self.child
    }
}

impl Drop for ReapedChild {
    fn drop(&mut self) {
        // If the child already exited, `try_wait` has reaped the zombie and there is
        // nothing to kill. Otherwise kill it and `wait` to collect it. Every call is
        // best-effort - a reaper whose child is already gone must never panic in `drop`.
        match self.child.try_wait() {
            Ok(Some(_)) => {}
            _ => {
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
        }
    }
}

#[cfg(test)]
mod supervised_lifecycle {
    //! Spec 19b, unit 3: the reaper mechanism reaps every long-lived `rigger` child
    //! after its guard is dropped / the driver exits, so a normally-finishing OR
    //! crashing agent leaves no orphaned `rigger` process. The standalone-`rigger dash`
    //! proof the criterion names lives in `tests/cli.rs` (it needs the compiled binary);
    //! these hermetic tests prove the SAME [`ReapedChild`] discipline generically, on a
    //! stand-in child on the CRASH path.
    use super::ReapedChild;
    use std::time::Duration;

    /// A real long-lived child that would outlive the test unless it is reaped. Its
    /// stdout is piped and never written to, so a reader on it blocks until the child
    /// EXITS (the child's write end closes -> EOF). That is a std-only, race-free "is
    /// it still alive?" probe that needs no `libc` (unavailable in the light lane).
    fn spawn_blocking_child() -> (std::process::Child, std::process::ChildStdout) {
        use std::process::{Command, Stdio};
        let mut child = Command::new("sleep")
            .arg("30")
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn a long-lived child");
        let out = child.stdout.take().expect("child stdout is piped");
        (child, out)
    }

    /// Watch a child's piped stdout on a helper thread: a `recv` that BLOCKS means the
    /// child is still alive (its write end is open); a `recv` that yields `0` means the
    /// child exited and its stdout reached EOF - i.e. it was reaped.
    fn watch_for_exit(mut out: std::process::ChildStdout) -> std::sync::mpsc::Receiver<usize> {
        use std::io::Read;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = [0u8; 1];
            let n = out.read(&mut buf).unwrap_or(0);
            let _ = tx.send(n);
        });
        rx
    }

    #[test]
    fn reaped_child_reaps_even_when_the_driver_panics() {
        let (child, out) = spawn_blocking_child();
        let exited = watch_for_exit(out);

        // A CRASHING driver (a panicking agent) still unwinds through the guard's Drop,
        // so the child is reaped on the crash path exactly as on the clean path.
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = ReapedChild::new(child);
            panic!("the driving agent crashed");
        }));
        assert!(panicked.is_err(), "the closure was expected to panic");

        let n = exited
            .recv_timeout(Duration::from_secs(5))
            .expect("a panic-unwound ReapedChild did not reap its process");
        assert_eq!(n, 0, "a reaped child's stdout should be at EOF");
    }
}
