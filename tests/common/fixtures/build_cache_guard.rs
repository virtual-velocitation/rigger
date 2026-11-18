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
/// refusal's OS error code ([`exclusive_probe`]).
pub fn try_exclusive_lock(path: &Path) -> Result<(), Option<i32>> {
    let guard = rigger::gate::open_build_cache_guard(path).expect("a second open of the guard");
    exclusive_probe(&guard)
}

/// An exclusive try-lock on the open `guard`: `Ok` when no process holds the guard, else the
/// refusal's OS error code. The lock, when taken, is released as the open closes.
pub fn exclusive_probe(guard: &std::fs::File) -> Result<(), Option<i32>> {
    fs2::FileExt::try_lock_exclusive(guard).map_err(|e| e.raw_os_error())
}

/// What [`try_exclusive_lock`] answers while another open holds the guard: the OS's contended
/// lock error.
pub fn contended() -> Result<(), Option<i32>> {
    Err(fs2::lock_contended_error().raw_os_error())
}
