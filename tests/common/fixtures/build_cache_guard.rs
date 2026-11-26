//! Shared build-cache guard fixtures: a gate command whose background child outlives the gate's
//! run, and the exclusive try-lock `rigger reset --build-cache` takes on the guard, with its
//! answer while another open holds the guard. Mounted only
//! where the library has `rigger::gate` - the gate crate and the root suites.

use std::path::Path;

/// A gate command that backgrounds a child with its stdout and stderr sent away from the gate's
/// pipes, so the command returns at once while the child lives on until the file at `hold` is
/// removed, bounded at 600 polls of 50 ms so it never outlives its test.
pub fn background_hold_command(hold: &Path) -> String {
    format!(
        "(i=0; while [ -e '{}' ] && [ $i -lt 600 ]; do sleep 0.05; i=$((i+1)); done) \
         >/dev/null 2>&1 &",
        hold.display()
    )
}

/// An exclusive try-lock on the guard at `path` through a second open of it, as
/// `rigger reset --build-cache` takes one: `Ok` when no process holds the guard, else the
/// refusal's error kind ([`exclusive_probe`]).
pub fn try_exclusive_lock(path: &Path) -> Result<(), std::io::ErrorKind> {
    let guard = rigger::gate::open_build_cache_guard(path).expect("a second open of the guard");
    exclusive_probe(&guard)
}

/// An exclusive try-lock on the open `guard`: `Ok` when no process holds the guard, else the
/// refusal's error kind. The lock, when taken, is released at once by an explicit unlock, as
/// `rigger reset --build-cache` releases its own, rather than by the open closing, so a copy of
/// the descriptor in a process another test thread forks, held until that process execs, never
/// keeps the guard past the probe.
pub fn exclusive_probe(guard: &std::fs::File) -> Result<(), std::io::ErrorKind> {
    guard
        .try_lock()
        .map_err(|e| std::io::Error::from(e).kind())?;
    guard.unlock().map_err(|e| e.kind())
}

/// What [`try_exclusive_lock`] answers while another open holds the guard: a contended lock.
pub fn contended() -> Result<(), std::io::ErrorKind> {
    Err(std::io::ErrorKind::WouldBlock)
}
