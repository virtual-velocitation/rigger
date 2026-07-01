//! Deterministic, controlled teardown of the ONNX Runtime / CUDA runtime, run on
//! the main thread BEFORE the process exits - so the corrupting atexit race that
//! `ort` + CUDA hits on Linux never happens.
//!
//! ## The upstream bug ([pykeio/ort#564])
//!
//! With the `cuda` execution provider on Linux, tearing the ONNX Runtime down at
//! process exit intermittently corrupts the glibc heap
//! (`malloc(): ... double linked list corrupted`, SIGABRT / exit 134). It is a
//! **known, still-open upstream bug**, closed *not-planned* because the maintainer
//! judged it not directly actionable by `ort`: CUDA is a single process-global
//! resource, and when the C++ ONNX Runtime provider and the CUDA runtime tear each
//! other's state down from *unordered C `atexit` / static destructors*, one frees a
//! block the other then reads. The upstream Valgrind trace pins it exactly: a
//! use-after-free inside `__run_exit_handlers`, in the ONNX-Runtime CUDA provider's
//! teardown, on a block already `free`d earlier in the same exit sequence. It still
//! reproduces on the latest `ort` (`2.0.0-rc.12` / ONNX Runtime 1.24.4), so **a
//! version bump does not fix it**; and rigger deliberately uses `load-dynamic` (see
//! `ort_runtime` - the whole GPU-discovery path depends on it), so the maintainer's
//! other suggested fix ("use the static prebuilt binaries") is off the table here.
//!
//! ## The fix this module implements
//!
//! Both the upstream maintainer and the reporter's minimal C reproduction converge on
//! the same real fix: **release the ONNX Runtime environment EXPLICITLY, before
//! `main` returns, instead of leaving it to the C `atexit` teardown.** The reporter's
//! C program that calls `ReleaseSession` then `ReleaseEnv` at the end of `main` -
//! rather than deferring to `atexit` - never crashes; the maintainer's own summary is
//! to expose "a `cleanup()` function that can be called to clean up ONNX before
//! `main()` exits so CUDA is no longer under contention". Run on the main thread while
//! the process is still healthy and single-threaded, ORT's provider + CUDA teardown
//! completes deterministically, so the racing atexit destructors find already-released
//! state and the double-free window is closed.
//!
//! [`release_ort_runtime`] is that `cleanup()`. `main` calls it, after dropping the
//! grounder (which drops the `fastembed` `TextEmbedding` and thus calls
//! `ReleaseSession`), on BOTH its success and its error exit paths - so unlike the old
//! `libc::_exit(0)` dodge it never leaves the error-after-embed path exposed, and it
//! runs every other destructor normally instead of skipping all of them.
//!
//! ## Why the explicit `ReleaseEnv` is sound here
//!
//! `ort` keeps the environment in a process-global `static` (`G_ENV`) holding an
//! `Arc<Environment>`. Rust does not run destructors for `static`s at program exit, so
//! that `Arc` is **leaked** - `Environment::drop` (which calls `ReleaseEnv`) never runs
//! on rigger's normal exit. This module therefore does the release itself: it takes the
//! `OrtEnv` pointer via the public [`ort::AsPointer`] API, drops its own `Arc` clone so
//! only the leaked `G_ENV` reference remains, and calls `ReleaseEnv` once through the
//! public raw API ([`ort::api`]/[`ort::sys`]). Because the `G_ENV` `Arc` is leaked and
//! never dereferenced again after this point (the process is on its way out), there is
//! no second `ReleaseEnv` and thus no double-free - the release happens exactly once,
//! deterministically, on the main thread.
//!
//! [pykeio/ort#564]: https://github.com/pykeio/ort/issues/564

use ort::AsPointer;

/// Release the process-global ONNX Runtime environment (and, with it, ORT's CUDA
/// provider state) deterministically, on the calling thread, before the process exits.
///
/// Call this from `main` AFTER the grounder - and hence the `fastembed`
/// `TextEmbedding` / ORT `Session` it owns - has been dropped, so `ReleaseSession` has
/// already run and only the environment remains to release. Mirrors the upstream-proven
/// `ReleaseSession` -> `ReleaseEnv` ordering from [pykeio/ort#564] that eliminates the
/// CUDA-teardown heap corruption.
///
/// A no-op when no environment was ever created (e.g. a run that never built a GPU
/// session): `get_environment` would create one on demand, so we deliberately do **not**
/// call it here - we only release an environment that already exists. It is safe to call
/// unconditionally: on the CPU / no-runtime path there is simply nothing to release.
///
/// [pykeio/ort#564]: https://github.com/pykeio/ort/issues/564
pub fn release_ort_runtime() {
    // A run that never built a turbovec model never loaded ORT. Skip: `ort::api()` and
    // `get_environment()` below would otherwise force a `dlopen` of a `libonnxruntime.so`
    // that may not exist (and, worse, `get_environment` would CREATE a fresh environment
    // on demand purely to tear it back down). `ort_was_initialized` is set only after
    // `TextEmbedding::try_new` succeeds - the one path that loads the runtime and commits
    // the env - so this is the precise "is there anything to release?" signal.
    if !crate::grounder::turbovec::ort_was_initialized() {
        return;
    }

    // Take the current global environment. We only reach here once a model was built,
    // which committed the env, so this returns the EXISTING `Arc` - it does not spin up a
    // fresh environment. (If a race somehow left none, the `Err`/None simply skips.)
    let Ok(env) = ort::environment::get_environment() else {
        return;
    };

    // Grab the raw `OrtEnv` pointer, then drop our own `Arc` clone. The only remaining
    // strong reference is the one leaked in ORT's `G_ENV` static, which Rust never
    // drops - so releasing the pointer below runs `ReleaseEnv` exactly once, with no
    // double-free (see the module docs for the full soundness argument).
    let env_ptr = env.ptr() as *mut ort::sys::OrtEnv;
    drop(env);

    // Release the environment through the public raw API. This is the deterministic,
    // main-thread teardown that replaces the racing atexit destructor - the crux of the
    // fix. `ReleaseEnv` is always present in a loaded `OrtApi`; if it somehow were not,
    // there is nothing to release and skipping is correct.
    // SAFETY: `env_ptr` is the live `OrtEnv` pointer just obtained from `ort`'s own
    // environment singleton (non-null per `AsPointer`), it has not been released before
    // (rigger never calls `ReleaseEnv` elsewhere, and `G_ENV`'s leaked `Arc` never
    // drops), and no ORT API runs concurrently: `main` calls this after the command has
    // returned and every grounder/session has been dropped, on the single main thread.
    if let Some(release_env) = ort::api().ReleaseEnv {
        unsafe { release_env(env_ptr) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// When no turbovec model was ever built - the CPU / grep-grounder / no-runtime path -
    /// `release_ort_runtime` must be a clean no-op: it must NOT force a `dlopen` of the
    /// runtime (which may be absent) nor create an environment purely to tear it back down.
    /// The `ort_was_initialized` guard is what buys that.
    ///
    /// The lib-test binary also hosts the turbovec model tests, and Rust runs tests in one
    /// process in an arbitrary order. If a model test already built a session in this
    /// process, ORT is initialized and its environment is LIVE - other in-process model
    /// tests may still use it - so we must NOT call `release_ort_runtime` (which would
    /// `ReleaseEnv` that live environment out from under them). We therefore only assert
    /// the no-op behavior on the uninitialized path, and skip cleanly otherwise: the actual
    /// release-with-a-live-session path is exercised end-to-end, in its own process, by the
    /// `tests/cli.rs` teardown tests.
    #[test]
    fn release_is_a_noop_when_ort_was_never_initialized() {
        if crate::grounder::turbovec::ort_was_initialized() {
            // A sibling model test already loaded ORT in this shared process; releasing its
            // live environment here would break the others. The uninitialized no-op path is
            // what this test targets - nothing to verify here, and the in-process order is
            // not ours to control.
            return;
        }
        // Must return cleanly on the uninitialized path WITHOUT reaching
        // `ort::api()`/`get_environment()` (which could panic trying to `dlopen` a runtime
        // that need not exist on a CPU box). The `ort_was_initialized` guard short-circuits.
        release_ort_runtime();
        // And it must not have loaded or committed anything: still uninitialized after.
        assert!(
            !crate::grounder::turbovec::ort_was_initialized(),
            "release_ort_runtime must not initialize ORT on the no-op (uninitialized) path"
        );
    }
}
