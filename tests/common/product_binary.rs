//! WHERE THE PRODUCT BINARY IS: the one authority every integration suite resolves the `rigger`
//! binary through ([`rigger_bin`]). Its own file, re-exported by `tests/common/mod.rs`, so a suite
//! that must build on every OS (`tests/bare_rigger_hooks_resolve_periphery.rs`) declares this file
//! alone (`#[path = "common/product_binary.rs"]`) rather than the whole shared module, whose
//! fixtures are Unix-only.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// The profile dir (`<target>/<profile>`) a cargo-run integration suite is running out of, read
/// from where its executable sits, or `None` when `test_exe` sits in neither place cargo runs an
/// integration suite from.
///
/// Cargo before 1.100 runs a suite from `<target>/<profile>/deps/<suite>-<hash>`. Cargo 1.100
/// made its build-unit layout the default: a suite runs from its own unit's output dir,
/// `<target>/<profile>/build/<package>/<hash>/out/<suite>-<hash>` (cargo's private opt-out still
/// gives the old shape). Each shape is matched whole, by structure; nothing is searched for
/// upward, and anything else is declined.
pub fn suite_profile_dir(test_exe: &Path) -> Option<&Path> {
    let dir = test_exe.parent()?;
    let name = dir.file_name()?;
    if name == "deps" {
        return dir.parent();
    }
    if name == "out" {
        let build = dir.parent()?.parent()?.parent()?;
        if build.file_name()? == "build" {
            return build.parent();
        }
    }
    None
}

/// The product binary that belongs to the target dir a test executable is running out of, or
/// `None` when `test_exe` is not a cargo-run integration suite.
///
/// Under either layout [`suite_profile_dir`] recognises, cargo uplifts the package's binaries to
/// the profile dir itself, `<target>/<profile>/`. Deriving the product from the RUNNING
/// executable therefore always names the target dir the test is actually in. (Cargo keeps suites
/// in its build dir and uplifts binaries to its target dir; the two are the same `target/` unless
/// a build dir is configured apart from it, and then the derived path names nothing and
/// [`rigger_bin`] falls back.)
///
/// WHY THIS EXISTS RATHER THAN `CARGO_BIN_EXE_rigger` ALONE. That macro expands at COMPILE time
/// to an absolute path into the target dir the suite was COMPILED under. Cargo's fingerprints do
/// not include the target-dir path, so a target dir that has MOVED - copied, restored, or
/// relocated by a build-cache lifecycle - is still judged fresh: nothing recompiles, the baked
/// path no longer exists, and every suite that spawns the product dies with
/// `Os { code: 2, kind: NotFound }` before asserting anything. That is not hypothetical here: a
/// workflow declaring `CARGO_TARGET_DIR` over `{build_cache}` gives each unit's gates a per-unit
/// one, and a relocated one turned a suite that is green cold into 0 passed / 3 failed with
/// exactly that error, while the recorded gate evidence never named it. Runtime derivation cannot
/// drift that way, because the answer is read from the same directory tree the test itself was
/// just loaded from.
///
/// It DECLINES (returns `None`) rather than guessing when the executable sits in neither layout -
/// a hand-run binary, a copied artifact - so [`rigger_bin`] can fall back honestly instead of
/// handing back a path nobody built.
pub fn product_binary_from(test_exe: &Path) -> Option<PathBuf> {
    suite_profile_dir(test_exe)
        .map(|profile| profile.join(format!("rigger{}", std::env::consts::EXE_SUFFIX)))
}

/// The compiled `rigger` binary this suite drives - THE one authority every integration suite
/// calls, so the location rule is stated once and cannot drift per suite.
///
/// Resolution order, and why: [`product_binary_from`] over the running test executable first,
/// because it follows a moved target dir; the compile-time `CARGO_BIN_EXE_rigger` second, which
/// is the correct answer whenever nothing moved and covers a runner that stages test executables
/// outside both cargo layouts, or a build dir cargo keeps apart from its target dir. This is the
/// SINGLE site in `tests/` allowed to spell that macro, pinned by
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
                derived.map(|p| p.display().to_string()).unwrap_or_else(|| {
                    "<none: the test executable is in neither cargo layout, \
                         <profile>/deps/ nor <profile>/build/<package>/<hash>/out/>"
                        .into()
                }),
                baked.display()
            )
        }
    }
}
