//! Periphery test for spec 89, criterion 2 (SCRATCH IS OUTSIDE THE STORE TREE) - the shared
//! test-only `XDG_CACHE_HOME` every `rigger_courier()` spawn is pinned to
//! ([`common::test_cache_home`], private to `tests/common/mod.rs`) must not leak the REAL
//! content a subprocess writes into it past the lifetime of the one thing that can reliably
//! reclaim it.
//!
//! `common::test_cache_home` used to be `static ... OnceLock<TempDir>` - ONE directory shared
//! by the WHOLE test binary. Rust statics are never dropped, under ANY process-exit path,
//! including `std::process::exit` (which the built-in libtest harness calls at the end of
//! every run) - so real content real `rigger_courier()`-spawned subprocesses wrote under it
//! (worktrees, agent scratch, the shared build cache) was permanently unreclaimed by the very
//! mechanism this fixture built specifically to keep it out of the operator's real
//! `~/.cache/rigger` (round 5 reject: `adv-u89c2-test-cache-home-static-never-dropped-leaks-
//! every-binary-run`, live-verified there by running a compiled test binary directly and
//! inspecting the filesystem after normal process exit).
//!
//! This proves the fix's actual mechanism directly, through [`common::default_scratch_root`] -
//! the same `pub` entry point every real fixture in this suite already calls - rather than
//! through a synthetic parallel. A worker thread stands in for one `#[test]` fn's own
//! dedicated OS thread (every `#[test]` runs on its own thread that the built-in harness joins
//! well before its final `process::exit` call - confirmed independently with a standalone
//! probe binary before writing this test, not assumed) and writes REAL content into the
//! default-rooted scratch directory it resolves, mirroring what a spawned `rigger` subprocess
//! leaves behind; once that thread has been joined, the content - and the directory that held
//! it - must be gone, with no separate cleanup call from this test or from anywhere else.

use std::path::PathBuf;

mod common;

#[test]
fn scratch_content_under_the_per_thread_cache_home_is_reclaimed_when_its_owning_thread_exits() {
    let fixture = tempfile::tempdir().expect("create a throwaway fixture root");
    let root = fixture.path().to_path_buf();

    let scratch: PathBuf = std::thread::spawn(move || {
        // Resolve the default scratch root exactly as a real fixture/test does on its own
        // dedicated thread, then write real content into it - standing in for what a spawned
        // `rigger` subprocess would leave behind (a worktree, agent scratch, a build-cache
        // entry). The point under test is the directory's OWN lifecycle, not any one
        // subprocess's behavior, so the write is done directly here rather than through a
        // real `rigger_courier()` spawn.
        let scratch = common::default_scratch_root(&root);
        std::fs::create_dir_all(&scratch).expect("create the resolved scratch directory");
        std::fs::write(scratch.join("marker"), b"real subprocess-written content")
            .expect("write real content into the resolved scratch directory");
        assert!(
            scratch.join("marker").exists(),
            "content must exist before the owning thread exits, or this test proves nothing"
        );
        scratch
    })
    .join()
    .expect("the worker thread must not panic");

    assert!(
        !scratch.exists(),
        "the scratch directory (and the real content written into it) must be reclaimed the \
         moment its owning thread exits, not survive for the rest of the process's life the \
         way a `static OnceLock<TempDir>` structurally never could reclaim it - see \
         adv-u89c2-test-cache-home-static-never-dropped-leaks-every-binary-run: {}",
        scratch.display()
    );
}
