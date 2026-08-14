//! Shared test support for the integration suites - compiled into each suite that declares
//! `mod common;`, so each suite uses the subset it needs (hence the module-wide `dead_code`
//! allowance: an item used by one suite is genuinely unused in the next).
//!
//! Today it holds exactly one concern: WHERE the product binary is. That concern earns a shared
//! home because it has sixteen readers and one correct answer, and because the obvious per-suite
//! spelling is wrong in a way no suite can see on its own (see [`product_binary_from`]).

#![allow(dead_code)]

use std::path::{Path, PathBuf};

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
