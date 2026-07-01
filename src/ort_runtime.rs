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
//! one), [`discover_dylib`] returns `None` and [`ensure_dylib_path`] leaves the env
//! untouched, so `ort`'s own default resolution (the bare `libonnxruntime.so` name
//! via the system loader) takes over. And when the CUDA runtime loads but the box has
//! no GPU, the grounder's `[CUDA, CPU]` EP list falls back to CPU.
//!
//! This module only ever ADDS a way to *find* the runtime; it never breaks a working
//! path. But finding one and being able to *load* it are different: if the discovery
//! chain comes up empty AND the system loader also cannot resolve a bare
//! `libonnxruntime.so`, then `ort`'s first use will `panic!` inside its
//! `lib_handle()` (its `dlopen` is `.unwrap_or_else(|e| panic!(...))`). That panic is
//! not catchable by `is_available().unwrap_or(false)`, so the grounder probes
//! [`dylib_is_resolvable`] BEFORE touching any `ort` API and fails with a clear error
//! instead of letting the raw panic escape. See `grounder::turbovec`.

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

/// The exact `libonnxruntime.so` path `ort` (under `load-dynamic`) will try to
/// `dlopen` on first use, mirroring `ort`'s own `dylib_path()` + `lib_handle()`
/// resolution so a caller can check loadability WITHOUT tripping `ort`'s internal
/// panic. Returns:
///   - the `ORT_DYLIB_PATH` value when set and non-empty (absolute is used verbatim;
///     a relative path is resolved against the executable's dir if that file exists,
///     else left relative for the system loader) - this is `ort`'s rule exactly;
///   - otherwise the bare default name `libonnxruntime.so`, likewise resolved against
///     the exe dir when present, else left bare for the system loader.
///
/// This does NOT set the env var and never touches an `ort` API; it only computes the
/// path so [`dylib_is_resolvable`] can probe it.
fn ort_dylib_candidate() -> PathBuf {
    // `ort`'s `dylib_path()`: honor a non-empty ORT_DYLIB_PATH, else the platform
    // default. We only ship on x86_64 Linux, so the default is `libonnxruntime.so`.
    let requested = match std::env::var(ORT_DYLIB_PATH) {
        Ok(s) if !s.is_empty() => PathBuf::from(s),
        _ => PathBuf::from("libonnxruntime.so"),
    };
    // `ort`'s `lib_handle()`: an absolute path is used as-is; a relative one is joined
    // to the executable's dir when THAT file exists, otherwise left relative so the
    // system loader (ldconfig search path) resolves it.
    if requested.is_absolute() {
        return requested;
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        let beside = dir.join(&requested);
        if beside.exists() {
            return beside;
        }
    }
    requested
}

/// Whether the ONNX Runtime dylib `ort` would `dlopen` can actually be loaded.
///
/// `ort`'s `lib_handle()` is `libloading::Library::new(path).unwrap_or_else(|e|
/// panic!(...))`, and `CUDAExecutionProvider::is_available()` reaches it, so a probe
/// like `is_available().unwrap_or(false)` cannot catch a missing runtime - it PANICS.
/// The grounder calls this first so it can degrade gracefully (a clear `Result::Err`)
/// instead of unwinding through that panic.
///
/// The check mirrors `ort`'s resolution ([`ort_dylib_candidate`]) and then attempts a
/// real `dlopen` of it via `libc`: an absolute/beside-exe path is opened by that path;
/// a bare name goes through the system loader exactly as `ort`'s `dlopen` would. The
/// handle is closed immediately - this only answers "would `ort`'s load succeed?",
/// with no lasting effect on the process. `RTLD_LAZY` defers symbol binding, so a
/// resolvable-but-quirky lib still counts as loadable (matching `ort`, which also just
/// opens it). A NULL handle (any dlopen failure) means not resolvable.
pub fn dylib_is_resolvable() -> bool {
    let candidate = ort_dylib_candidate();
    // If the candidate is an explicit path (absolute, or resolved beside the exe) it
    // must exist to be openable; a bare name is left to the loader's search path.
    if (candidate.is_absolute() || candidate.components().count() > 1) && !candidate.exists() {
        return false;
    }
    let Ok(c_path) = std::ffi::CString::new(candidate.as_os_str().as_encoded_bytes()) else {
        return false; // a path with an interior NUL can never be a real dylib path
    };
    // SAFETY: `dlopen`/`dlclose` are plain libc calls. `c_path` is a valid, NUL-
    // terminated C string that outlives the `dlopen` call. On success we immediately
    // `dlclose` the handle we opened, so this probe leaves no library mapped by us
    // (the OS refcounts; `ort`'s later real load is unaffected). On failure `dlopen`
    // returns NULL, which we report as "not resolvable".
    unsafe {
        let handle = libc::dlopen(c_path.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL);
        if handle.is_null() {
            return false;
        }
        libc::dlclose(handle);
    }
    true
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

    /// An absolute `ORT_DYLIB_PATH` that names a real, loadable shared object is
    /// reported resolvable, mirroring `ort`'s `dlopen` of the same path - WITHOUT
    /// tripping the panic `is_available()` would. We point it at a genuine system
    /// `.so` (`libc` itself), so the probe's real `dlopen` succeeds. `serial` because
    /// it mutates the process-wide `ORT_DYLIB_PATH` env var.
    #[test]
    #[serial_test::serial(ort_dylib_env)]
    fn resolvable_when_env_points_at_a_real_shared_object() {
        // A shared object every Linux box has and that `dlopen` can open by absolute
        // path. Pick the first that exists so the test is host-robust.
        let real_so = ["/lib/x86_64-linux-gnu/libc.so.6", "/usr/lib/libc.so.6"]
            .into_iter()
            .map(PathBuf::from)
            .find(|p| p.exists());
        let Some(real_so) = real_so else {
            return; // no known libc path on this host; skip rather than false-fail
        };
        let prev = std::env::var_os(ORT_DYLIB_PATH);
        // SAFETY: single-threaded test body; `#[serial]` keeps other env mutators out.
        unsafe { std::env::set_var(ORT_DYLIB_PATH, &real_so) };
        assert!(
            dylib_is_resolvable(),
            "an ORT_DYLIB_PATH pointing at a real, loadable .so must be resolvable"
        );
        // SAFETY: restore the prior value under the same serialized section.
        unsafe {
            match prev {
                Some(v) => std::env::set_var(ORT_DYLIB_PATH, v),
                None => std::env::remove_var(ORT_DYLIB_PATH),
            }
        }
    }

    /// An absolute `ORT_DYLIB_PATH` naming a file that does not exist is NOT
    /// resolvable - the probe returns `false` (a clean bool) rather than letting a
    /// later `ort` call panic on the missing dylib. This is the cleared-cache edge the
    /// grounder must degrade on.
    #[test]
    #[serial_test::serial(ort_dylib_env)]
    fn not_resolvable_when_env_points_at_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("libonnxruntime.so"); // never created
        let prev = std::env::var_os(ORT_DYLIB_PATH);
        // SAFETY: single-threaded test body; `#[serial]` serializes env mutators.
        unsafe { std::env::set_var(ORT_DYLIB_PATH, &missing) };
        assert!(
            !dylib_is_resolvable(),
            "an ORT_DYLIB_PATH pointing at a non-existent file must be unresolvable, not panic"
        );
        // SAFETY: restore the prior value under the same serialized section.
        unsafe {
            match prev {
                Some(v) => std::env::set_var(ORT_DYLIB_PATH, v),
                None => std::env::remove_var(ORT_DYLIB_PATH),
            }
        }
    }

    /// The candidate path mirrors `ort`'s own resolution: a set, non-empty absolute
    /// `ORT_DYLIB_PATH` is honored verbatim; unset falls back to the default name
    /// `libonnxruntime.so` - resolved against the exe dir if that file exists (exactly
    /// what `ort`'s `lib_handle` does), else left bare for the system loader.
    #[test]
    #[serial_test::serial(ort_dylib_env)]
    fn candidate_mirrors_ort_resolution() {
        let prev = std::env::var_os(ORT_DYLIB_PATH);
        // An absolute env value is used verbatim.
        // SAFETY: serialized env mutation in a single-threaded test.
        unsafe { std::env::set_var(ORT_DYLIB_PATH, "/opt/ort/libonnxruntime.so") };
        assert_eq!(
            ort_dylib_candidate(),
            PathBuf::from("/opt/ort/libonnxruntime.so"),
            "an absolute ORT_DYLIB_PATH must be honored verbatim, like ort does"
        );
        // Unset: the candidate is the default name, EITHER left bare OR resolved to the
        // exe-dir copy when one exists there (the test binary's deps/ dir may hold a real
        // libonnxruntime.so). Both are `ort`'s exact resolution; the invariant is the
        // FILE NAME, and that an explicit (multi-component) path only appears when it
        // actually exists on disk.
        // SAFETY: serialized env mutation in a single-threaded test.
        unsafe { std::env::remove_var(ORT_DYLIB_PATH) };
        let candidate = ort_dylib_candidate();
        assert_eq!(
            candidate.file_name().and_then(|n| n.to_str()),
            Some("libonnxruntime.so"),
            "the unset default must resolve to the libonnxruntime.so name"
        );
        let is_bare = candidate == Path::new("libonnxruntime.so");
        assert!(
            is_bare || (candidate.components().count() > 1 && candidate.exists()),
            "unset must be the bare name, or an existing beside-exe path - never a \
             non-existent explicit path; got {candidate:?}"
        );
        // SAFETY: restore prior value.
        unsafe {
            match prev {
                Some(v) => std::env::set_var(ORT_DYLIB_PATH, v),
                None => std::env::remove_var(ORT_DYLIB_PATH),
            }
        }
    }
}
