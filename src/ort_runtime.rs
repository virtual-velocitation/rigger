//! Point `ort` at a CUDA-enabled ONNX Runtime `.so` to `dlopen`, so the turbovec
//! grounder's embeddings run on the GPU with NO user-set env - for BOTH the
//! standalone (`./target/.../rigger`) and the `cargo install`ed (`~/.cargo/bin/rigger`)
//! binary.
//!
//! ## Why this exists
//!
//! `ort` is built with `load-dynamic` (see `Cargo.toml`): at runtime it `dlopen`s an
//! external `libonnxruntime.so` chosen by the `ORT_DYLIB_PATH` env var rather than
//! linking a bundled static lib. The `cuda` + `download-binaries` features make
//! `ort-sys` FETCH the CUDA-enabled runtime into its download cache
//! (`~/.cache/ort.pyke.io/dfbin/<target-triple>/<hash>/onnxruntime/lib/libonnxruntime.so`),
//! but with `load-dynamic` nothing tells `ort` where that file is - the built binary
//! has no `$ORIGIN` RUNPATH, and a `cargo install`ed binary has neither the lib beside
//! it nor the cache path. So without help `ort` fails with
//! `libonnxruntime.so: cannot open shared object file`.
//!
//! [`ensure_dylib_path`] closes that gap: BEFORE `ort` is first used, it sets
//! `ORT_DYLIB_PATH` to a discovered CUDA-enabled `libonnxruntime.so`. `ort` then
//! `dlopen`s that exact file, and finds its sibling execution-provider `.so`s
//! (`libonnxruntime_providers_cuda.so`, ...) in the same directory. Those providers'
//! NEEDED CUDA 12 + cuDNN 9 libs (`libcudart.so.12`, `libcudnn.so.9`, ...) live in
//! `/lib/x86_64-linux-gnu` / `/usr/local/cuda/.../lib` and are ldconfig-visible on a
//! GPU box, so the dynamic loader resolves them with no `LD_LIBRARY_PATH` help. That
//! is why setting `ORT_DYLIB_PATH` alone suffices - no re-exec.
//!
//! ## Graceful fallback
//!
//! When no `libonnxruntime.so` is discovered (a box where `ort-sys` never downloaded
//! one), [`discover_dylib`] returns `None`, [`ensure_dylib_path`] leaves the env
//! untouched, and `ort`'s own default resolution takes over. And when the CUDA
//! runtime loads but the box has no GPU, the grounder's `[CUDA, CPU]` EP list falls
//! back to CPU. So this module only ever ADDS a way to find the runtime; it never
//! breaks a working path.

use std::path::{Path, PathBuf};

/// The env var `ort` reads (under `load-dynamic`) to pick which `libonnxruntime.so`
/// to `dlopen`. We set it to a discovered CUDA-enabled runtime.
const ORT_DYLIB_PATH: &str = "ORT_DYLIB_PATH";

/// The rustc target triple `ort-sys` names the per-triple subdir of its download
/// cache with. We are the x86_64 Linux GPU deployment; a non-matching host simply
/// finds nothing under this subdir and falls through to `ort`'s own resolution.
const TARGET_TRIPLE: &str = "x86_64-unknown-linux-gnu";

/// Point `ORT_DYLIB_PATH` at a discovered CUDA-enabled `libonnxruntime.so` so `ort`
/// can `dlopen` the runtime, UNLESS the caller already set it. Call this FIRST in
/// `main`, before anything constructs a grounder (`ort` reads `ORT_DYLIB_PATH` when it
/// first loads the runtime).
///
/// Discovery order (first hit wins):
///   (a) an already-set `ORT_DYLIB_PATH` - the user/CI is driving runtime selection by
///       hand, so respect it and do nothing;
///   (b) `libonnxruntime.so` next to the current executable (a deployment that shipped
///       the runtime beside the binary);
///   (c) the newest `libonnxruntime.so` in the `ort-sys` download cache
///       (`~/.cache/ort.pyke.io/dfbin/<triple>/*/onnxruntime/lib/`), where the `cuda` +
///       `download-binaries` features put the CUDA-enabled runtime.
///
/// Harmless when the feature is off or no runtime is present: it just leaves the env
/// as-is and the normal `ort` path applies.
///
/// # Safety
/// Mutates a process env var (`ORT_DYLIB_PATH`), which `ort` READS lazily when it
/// first loads the runtime. The caller MUST guarantee no other thread reads the
/// environment concurrently with this write - in particular, no other thread may be
/// constructing an `ort` session. Two call sites satisfy that: `main` calls it as its
/// very first statement, before any thread is spawned; and `Turbovec::construct` calls
/// it while holding the process-wide `CONSTRUCT_MU`, which every model construction
/// also holds, so no concurrent session load (and thus no concurrent env read) can race
/// the write.
pub unsafe fn ensure_dylib_path() {
    // (a) The user/CI already chose a runtime: never override it.
    if std::env::var_os(ORT_DYLIB_PATH).is_some() {
        return;
    }
    let Some(dylib) = discover_dylib() else {
        return; // nothing to point at -> leave ort's default resolution in charge.
    };
    // SAFETY: forwarded to the caller's contract above (single-threaded at call time).
    unsafe { std::env::set_var(ORT_DYLIB_PATH, &dylib) };
}

/// Find a CUDA-enabled `libonnxruntime.so` to hand `ort`, without consulting or
/// mutating any env var. Returns the first of:
///   (b) `libonnxruntime.so` in the current executable's directory;
///   (c) the newest `libonnxruntime.so` in the `ort-sys` dfbin download cache.
/// `None` when neither exists (the CPU/no-runtime box), so the caller leaves
/// `ORT_DYLIB_PATH` unset.
fn discover_dylib() -> Option<PathBuf> {
    beside_current_exe().or_else(dfbin_cache_dylib)
}

/// `libonnxruntime.so` sitting next to the running binary, if any. This is the
/// deployment where the runtime was copied beside the executable; a bare filename
/// match is enough since it is the executable's own directory.
fn beside_current_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let candidate = dir.join("libonnxruntime.so");
    candidate.is_file().then_some(candidate)
}

/// The newest `libonnxruntime.so` in the `ort-sys` download cache:
/// `~/.cache/ort.pyke.io/dfbin/<triple>/<hash>/onnxruntime/lib/libonnxruntime.so`.
/// The `cuda` + `download-binaries` features put the CUDA-enabled runtime here. There
/// can be several hashed dirs (e.g. a prior CPU-only download); we pick the
/// newest-mtime `libonnxruntime.so` so a fresh CUDA download wins over a stale one.
fn dfbin_cache_dylib() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let dfbin = Path::new(&home)
        .join(".cache")
        .join("ort.pyke.io")
        .join("dfbin")
        .join(TARGET_TRIPLE);
    newest_dylib_under(&dfbin)
}

/// Walk `<dfbin>/<hash>/onnxruntime/lib/` for `libonnxruntime.so`, returning the one
/// with the newest modification time (so a fresh CUDA-enabled download beats a stale
/// entry). `None` if the cache dir is absent or holds no runtime `.so`.
fn newest_dylib_under(dfbin_triple: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dfbin_triple).ok()?;
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in entries.flatten() {
        let so = entry
            .path()
            .join("onnxruntime")
            .join("lib")
            .join("libonnxruntime.so");
        if !so.is_file() {
            continue;
        }
        // Prefer the newest mtime; fall back to UNIX_EPOCH if a stat somehow fails so a
        // readable file is never silently dropped.
        let mtime = std::fs::metadata(&so)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        match &best {
            Some((best_mtime, _)) if *best_mtime >= mtime => {}
            _ => best = Some((mtime, so)),
        }
    }
    best.map(|(_, path)| path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A dfbin cache tree with one hashed runtime dir is discovered: the helper walks
    /// `<hash>/onnxruntime/lib/libonnxruntime.so` and returns that `.so`. This is the
    /// `cargo install`ed-binary path - no lib beside the exe, so the cache is the source.
    #[test]
    fn discovers_dylib_in_dfbin_cache() {
        let dir = tempfile::tempdir().unwrap();
        let lib = dir.path().join("DEADBEEF").join("onnxruntime").join("lib");
        fs::create_dir_all(&lib).unwrap();
        let so = lib.join("libonnxruntime.so");
        fs::write(&so, b"\x7fELF").unwrap();
        assert_eq!(
            newest_dylib_under(dir.path()),
            Some(so),
            "the runtime .so under a hashed cache dir must be discovered"
        );
    }

    /// With several hashed dirs, the NEWEST-mtime `libonnxruntime.so` wins - a fresh
    /// CUDA-enabled download beats a stale earlier one. We write the "older" file, wait
    /// for the clock to advance past the filesystem's mtime granularity, then write the
    /// "newer" one, and assert the newer path is returned.
    #[test]
    fn newest_cache_entry_wins() {
        let dir = tempfile::tempdir().unwrap();
        let mk = |hash: &str| {
            let lib = dir.path().join(hash).join("onnxruntime").join("lib");
            fs::create_dir_all(&lib).unwrap();
            let so = lib.join("libonnxruntime.so");
            fs::write(&so, b"\x7fELF").unwrap();
            so
        };
        let older = mk("OLD");
        // Sleep past coarse (e.g. 1s) filesystem mtime granularity so `newer` is
        // unambiguously later than `older`.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let newer = mk("NEW");
        assert_eq!(
            newest_dylib_under(dir.path()),
            Some(newer),
            "the newest-mtime runtime .so must win over an older one"
        );
        // Sanity: `older` really is the earlier file and would NOT be chosen.
        assert!(
            older.exists(),
            "the older cache entry still exists but must not be selected"
        );
    }

    /// An empty / absent cache dir yields no runtime, so the caller leaves
    /// `ORT_DYLIB_PATH` unset and `ort`'s own resolution takes over (CPU / no-GPU box).
    #[test]
    fn absent_cache_yields_no_dylib() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            newest_dylib_under(&dir.path().join("does-not-exist")).is_none(),
            "an absent cache dir must yield no runtime"
        );
        assert!(
            newest_dylib_under(dir.path()).is_none(),
            "an empty cache dir (no hashed runtime subtree) must yield no runtime"
        );
    }

    /// A hashed dir WITHOUT the `onnxruntime/lib/libonnxruntime.so` file is ignored -
    /// only a real runtime tree counts, never a stray directory.
    #[test]
    fn hashed_dir_without_runtime_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("EMPTY").join("onnxruntime")).unwrap();
        assert!(
            newest_dylib_under(dir.path()).is_none(),
            "a hashed dir with no libonnxruntime.so must not be discovered"
        );
    }
}
