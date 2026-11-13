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

use crate::common;

use common::repo::{for_each_rs_file, repo_root};
use std::path::{Path, PathBuf};

/// The derivation, as a pure function of where the test executable sits, over both layouts cargo
/// runs an integration suite from: `<target>/<profile>/deps/<suite>-<hash>` before cargo 1.100,
/// and `<target>/<profile>/build/<package>/<hash>/out/<suite>-<hash>` from 1.100 on. Either way
/// cargo uplifts the product binary to `<target>/<profile>/`, so a moved target dir resolves to
/// its own product, never to the one the suite was compiled beside. Any other path is not a
/// cargo-run suite, and the derivation declines rather than invent a sibling path, which lets
/// [`common::rigger_bin`] fall back honestly instead of returning a path nobody built. The rule
/// is expressed over a path rather than over the process so it can be pinned without moving a
/// 174 MiB binary around.
#[test]
fn the_product_binary_is_derived_from_either_cargo_layout_and_from_nothing_else() {
    let product = |profile: &str| {
        Some(PathBuf::from(format!(
            "{profile}/rigger{}",
            std::env::consts::EXE_SUFFIX
        )))
    };
    let rows = [
        (
            "the deps layout",
            "/anywhere/target/debug/deps/change_path_revert_periphery-7183638cf17279b4",
            product("/anywhere/target/debug"),
        ),
        (
            "the deps layout in a moved target dir",
            "/b/other-target/debug/deps/cli-1",
            product("/b/other-target/debug"),
        ),
        (
            "the deps layout under a target tuple",
            "/t/target/x86_64-unknown-linux-gnu/release/deps/all-1",
            product("/t/target/x86_64-unknown-linux-gnu/release"),
        ),
        (
            "the build-unit layout",
            "/anywhere/target/debug/build/rigger/92a7a7b308420ffa/out/\
             product_binary_authority_periphery-92a7a7b308420ffa",
            product("/anywhere/target/debug"),
        ),
        (
            "the build-unit layout in a moved target dir",
            "/b/other-target/debug/build/rigger/1ff6b22e8039aa84/out/all-1ff6b22e8039aa84",
            product("/b/other-target/debug"),
        ),
        (
            "the build-unit layout under a target tuple",
            "/t/target/x86_64-unknown-linux-gnu/release/build/rigger/1ff6b22e8039aa84/out/all-1",
            product("/t/target/x86_64-unknown-linux-gnu/release"),
        ),
        ("an installed binary", "/usr/local/bin/whatever", None),
        ("a bare file name", "rigger", None),
        ("a short relative path", "out/all-1", None),
        (
            "an out dir directly in the profile dir",
            "/anywhere/target/debug/out/all-1",
            None,
        ),
        (
            "the deps layout's build-script out dir",
            "/anywhere/target/debug/build/rigger-1ff6b22e8039aa84/out/all-1",
            None,
        ),
        (
            "an out dir three below a dir other than build",
            "/anywhere/target/debug/examples/rigger/1ff6b22e8039aa84/out/all-1",
            None,
        ),
    ];
    for (layout, exe, expected) in rows {
        assert_eq!(
            common::product_binary_from(Path::new(exe)),
            expected,
            "{layout}: {exe}"
        );
    }
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
    let tests_dir = repo_root().join("tests");
    let mut offenders: Vec<String> = Vec::new();
    let mut authority_sites = 0usize;
    for_each_rs_file(&tests_dir, &mut |path, source| {
        let rel = path
            .strip_prefix(&tests_dir)
            .expect("a path under tests/")
            .to_string_lossy()
            .into_owned();
        if rel == "product_binary_location.rs" {
            return;
        }
        if rel == "common/mod.rs" {
            // The authority may DISCUSS the macro in prose; what is counted is the one
            // invocation it is allowed to make.
            authority_sites += source.matches(&invocation).count();
            return;
        }
        let hits = source.matches(&needle).count();
        if hits > 0 {
            offenders.push(format!("{rel} ({hits})"));
        }
    });
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
