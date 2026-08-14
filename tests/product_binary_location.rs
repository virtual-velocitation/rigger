//! Where an integration suite finds the product binary, and why that is one authority.
//!
//! A suite that drives the shipped CLI has to spawn `rigger`. The obvious spelling is Cargo's
//! `CARGO_BIN_EXE_rigger`, but that is an ABSOLUTE path baked into the test binary at COMPILE
//! time as `<target-dir>/<profile>/rigger`. Cargo's fingerprints do not include the target-dir
//! path, so a target dir that has MOVED - copied, restored, or relocated by a build-cache
//! lifecycle - is still judged fresh, and every suite compiled under the old path then dies with
//! `Os { code: 2, kind: NotFound }` the moment it spawns the product. Reproduced here at the
//! unit tip: a cold per-unit target dir runs the whole suite green, and the SAME dir moved one
//! directory over turns `tests/change_path_revert_periphery.rs` into 0 passed / 3 failed with
//! exactly that error.
//!
//! [`common::product_binary_from`] removes the coupling: it derives the product path from the
//! RUNNING test executable, which cargo places at `<target>/<profile>/deps/<suite>-<hash>`, so
//! the answer always names the target dir the test is actually running out of. These tests pin
//! that derivation and pin that every suite goes through the one authority.

mod common;

use std::path::{Path, PathBuf};

/// The derivation, as a pure function of where the test executable sits: cargo runs an
/// integration suite from `<target>/<profile>/deps/`, and uplifts the product binary one
/// directory up at `<target>/<profile>/`. This is the whole rule, and it is deliberately
/// expressed over a path rather than over the process so it can be pinned without moving a
/// 174 MiB binary around.
#[test]
fn the_product_binary_sits_one_directory_above_the_running_suite() {
    let derived = common::product_binary_from(Path::new(
        "/anywhere/target/debug/deps/change_path_revert_periphery-7183638cf17279b4",
    ));
    assert_eq!(
        derived,
        Some(PathBuf::from(format!(
            "/anywhere/target/debug/rigger{}",
            std::env::consts::EXE_SUFFIX
        ))),
        "the product is the uplifted binary in the same profile dir as the suite's `deps/`"
    );
}

/// The derivation follows a MOVED target dir, which is the whole point: the same suite name
/// under a different target root resolves to that root's product, never to the one it was
/// compiled beside. This is the assertion that would have caught the gate failure.
#[test]
fn a_moved_target_dir_resolves_to_its_own_product_not_the_compiled_beside_one() {
    let before = common::product_binary_from(Path::new("/a/target/debug/deps/cli-1"));
    let after = common::product_binary_from(Path::new("/b/other-target/debug/deps/cli-1"));
    assert_ne!(before, after, "a moved target dir must resolve differently");
    assert_eq!(
        after,
        Some(PathBuf::from(format!(
            "/b/other-target/debug/rigger{}",
            std::env::consts::EXE_SUFFIX
        )))
    );
}

/// An executable that is not sitting in a `deps/` dir is not a cargo-run integration suite, so
/// the derivation declines rather than inventing a sibling path. Declining is what lets
/// [`common::rigger_bin`] fall back honestly instead of returning a path nobody built.
#[test]
fn an_executable_outside_a_deps_dir_yields_no_derivation() {
    assert_eq!(
        common::product_binary_from(Path::new("/usr/local/bin/whatever")),
        None
    );
    assert_eq!(common::product_binary_from(Path::new("rigger")), None);
}

/// The live resolution, exercised the way every suite exercises it: the path this process
/// resolves must exist and must be the product in THIS run's target dir.
#[test]
fn the_resolved_product_binary_exists_in_the_target_dir_this_suite_runs_from() {
    let bin = common::rigger_bin();
    assert!(
        bin.exists(),
        "resolved product binary does not exist: {}",
        bin.display()
    );
    let exe = std::env::current_exe().expect("the running test executable");
    let profile_dir = exe
        .parent()
        .and_then(Path::parent)
        .expect("<target>/<profile>/deps/<suite>");
    assert_eq!(
        bin.parent(),
        Some(profile_dir),
        "the product must come from the target dir this suite is running out of"
    );
}

/// The drift assertion: `CARGO_BIN_EXE_rigger` is spelled in exactly ONE place, the authority in
/// `tests/common/mod.rs` that uses it as a documented fallback. Every other suite goes through
/// [`common::rigger_bin`]. Without this pin the fragile spelling grows back one suite at a time,
/// which is how it reached sixteen files.
///
/// The needle is assembled at runtime so this file - the only suite that must name the macro to
/// forbid it - does not match itself.
#[test]
fn no_suite_bakes_the_product_path_at_compile_time_except_the_one_authority() {
    let needle = format!("CARGO_BIN{}EXE_rigger", "_");
    let invocation = format!("env!(\"{needle}\")");
    let tests_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut offenders: Vec<String> = Vec::new();
    let mut authority_sites = 0usize;
    let mut walk = vec![tests_dir.clone()];
    while let Some(dir) = walk.pop() {
        for entry in std::fs::read_dir(&dir).expect("the tests dir is readable") {
            let path = entry.expect("a readable dir entry").path();
            if path.is_dir() {
                walk.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let rel = path
                .strip_prefix(&tests_dir)
                .expect("a path under tests/")
                .to_string_lossy()
                .into_owned();
            if rel == "product_binary_location.rs" {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("a readable test source");
            if rel == "common/mod.rs" {
                // The authority may DISCUSS the macro in prose; what is counted is the one
                // invocation it is allowed to make.
                authority_sites += source.matches(&invocation).count();
                continue;
            }
            let hits = source.matches(&needle).count();
            if hits > 0 {
                offenders.push(format!("{rel} ({hits})"));
            }
        }
    }
    offenders.sort();
    assert!(
        offenders.is_empty(),
        "these suites bake the product path at compile time instead of calling \
         common::rigger_bin(): {}",
        offenders.join(", ")
    );
    assert_eq!(
        authority_sites, 1,
        "the fallback belongs at exactly one site in tests/common/mod.rs"
    );
}
