//! Periphery (integration) test for the ONE product-binary authority every suite now calls:
//! `common::rigger_bin`.
//!
//! WHAT THE INSIDE-OUT TESTS ARE STRUCTURALLY BLIND TO. `tests/product_binary_location.rs` pins
//! the pure derivation (`common::product_binary_from` over a path), pins that the derivation
//! declines outside a `deps/` dir, pins that the resolved binary exists in the target dir this
//! process runs from, and pins that no other suite bakes the compile-time path. None of those can
//! see the property the authority is actually built around: its RESOLUTION ORDER.
//!
//! (This file names the compile-time macro nowhere, on purpose: `tests/product_binary_location.rs`
//! forbids that spelling everywhere but the authority itself, by text, so the fragile spelling
//! cannot grow back. "The baked candidate" below is that macro's expansion.)
//!
//! `rigger_bin` has two candidates - the path DERIVED from the running test executable, and the
//! absolute path BAKED IN at compile time by cargo's bin-exe macro - and it is documented to prefer
//! the derived one, because cargo's fingerprints do not include the target-dir path, so a target
//! dir that was copied or restored is judged fresh while the baked path still names the ORIGINAL
//! dir. Under an ordinary `cargo test` the two candidates are the SAME path, so every assertion
//! that only inspects the answer holds for either order, and a resolver that consulted the baked
//! path first would pass the whole suite while handing back a product from a target dir this
//! process is not running out of.
//!
//! This suite makes the two candidates BOTH EXIST AND DIFFER - the copied/restored target dir,
//! which is the case the order exists for - and observes which one the authority returns. It does
//! that the only way an external consumer can: it stages a second target dir beside the real one,
//! links this very test executable and the product into it, and re-runs one ignored test of this
//! same suite from there, so the resolution runs in a process whose `current_exe()` really is the
//! moved one. Nothing is faked and nothing is injected.
//!
//! Cost: the staging is hard links (a fallback copy only if the link is refused), and the whole
//! probe lives inside the target dir it is probing, so it never leaves that tree and never lands
//! on another filesystem. It compiles unconditionally, so it runs in both feature lanes.

mod common;

use std::path::{Path, PathBuf};

/// How the parent hands the child the one answer it must produce. An environment variable, not an
/// argument, because the child is a libtest binary: its argv belongs to the harness.
const EXPECTED_PRODUCT_ENV: &str = "RIGGER_PERIPHERY_EXPECTED_PRODUCT";

/// The name of the child test, spelled once so the spawn and the definition cannot drift.
const CHILD_TEST: &str = "the_resolved_product_is_the_one_beside_the_running_executable";

/// The product binary name on this platform.
fn product_file_name() -> String {
    format!("rigger{}", std::env::consts::EXE_SUFFIX)
}

/// Stage `src` at `dst` without paying for a 174 MiB copy when the filesystem will share the
/// inode. Both paths are inside the same target dir, so the link is the normal outcome; the copy
/// is the honest fallback for a filesystem that refuses links.
fn stage(src: &Path, dst: &Path) {
    if std::fs::hard_link(src, dst).is_ok() {
        return;
    }
    std::fs::copy(src, dst).unwrap_or_else(|e| {
        panic!(
            "could not stage {} at {}: {e}",
            src.display(),
            dst.display()
        )
    });
}

/// THE CHILD. Driven only as a subprocess by the test below, from a staged target dir, with the
/// answer it must produce handed in through the environment.
///
/// It is `#[ignore]`d because it is meaningless in the parent's own process: there the derived and
/// baked candidates are the same path, so it would assert nothing. Run on its own it fails loudly
/// on the missing variable rather than passing vacuously.
#[test]
#[ignore = "driven as a subprocess from a staged target dir by the_authority_prefers_the_target_dir_it_is_running_out_of"]
fn the_resolved_product_is_the_one_beside_the_running_executable() {
    let expected = std::env::var(EXPECTED_PRODUCT_ENV).unwrap_or_else(|_| {
        panic!(
            "{EXPECTED_PRODUCT_ENV} is not set: this test is driven as a subprocess by \
             the_authority_prefers_the_target_dir_it_is_running_out_of, which stages a second \
             target dir and hands in the product that dir holds. It asserts nothing on its own"
        )
    });
    let resolved = common::rigger_bin();
    assert_eq!(
        resolved,
        PathBuf::from(&expected),
        "the authority must resolve the product of the target dir THIS executable is running out \
         of. It returned {} while this process runs from a target dir holding {expected}, which \
         means the compile-time CARGO_BIN_EXE path won over the derived one - the exact inversion \
         that makes a copied or restored target dir drive a product from the dir it was compiled \
         under instead of the dir it is in",
        resolved.display()
    );
}

/// THE ORDER, observed from outside with both candidates present and different.
///
/// The staging reproduces a restored build cache: the original target dir is untouched (so the
/// baked path still resolves to a real file), and a second one holds this
/// suite's executable and its own copy of the product. A resolver that preferred the baked path
/// would answer with the ORIGINAL product - existing, runnable, and from the wrong tree - and no
/// assertion about the answer's existence could tell the difference.
///
/// Both preconditions are asserted before the observation, so the proof can never pass vacuously:
/// the baked candidate must really exist (otherwise a baked-first resolver would fall through to
/// the derived one and look correct), and the staged product must be a different path from it.
#[test]
fn the_authority_prefers_the_target_dir_it_is_running_out_of() {
    let exe = std::env::current_exe().expect("the running test executable");
    let deps = exe.parent().expect("<target>/<profile>/deps");
    assert_eq!(
        deps.file_name().and_then(|n| n.to_str()),
        Some("deps"),
        "this proof stages a target dir around the running suite, so the suite must be running \
         from a cargo `deps/` dir; it is at {}",
        exe.display()
    );
    let profile = deps.parent().expect("<target>/<profile>");
    let target = profile.parent().expect("<target>");

    let baked = profile.join(product_file_name());
    assert!(
        baked.exists(),
        "the compiled-beside product must exist for this to be a proof of ORDER: with only one \
         candidate present, a resolver that consults them in either order answers the same. \
         Expected it at {}",
        baked.display()
    );

    // The staged target dir lives INSIDE the target dir under probe, so the links never cross a
    // filesystem and the probe never escapes the tree cargo already owns.
    let probe = target.join(format!("periphery-staged-target-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&probe);
    let staged_profile = probe.join(profile.file_name().expect("<profile>"));
    let staged_deps = staged_profile.join("deps");
    std::fs::create_dir_all(&staged_deps)
        .unwrap_or_else(|e| panic!("could not stage {}: {e}", staged_deps.display()));

    let staged_product = staged_profile.join(product_file_name());
    stage(&baked, &staged_product);
    let staged_exe = staged_deps.join(exe.file_name().expect("the suite's file name"));
    stage(&exe, &staged_exe);

    assert_ne!(
        staged_product, baked,
        "the two candidates must be different paths for the order to be observable"
    );

    let outcome = std::process::Command::new(&staged_exe)
        .args(["--exact", "--ignored", "--nocapture", CHILD_TEST])
        .env(EXPECTED_PRODUCT_ENV, &staged_product)
        .output();

    // Clean up before asserting, so a failing observation still leaves the target dir as it was.
    let _ = std::fs::remove_dir_all(&probe);

    let outcome = outcome.unwrap_or_else(|e| {
        panic!(
            "could not run the staged copy of this suite at {}: {e}",
            staged_exe.display()
        )
    });
    let stdout = String::from_utf8_lossy(&outcome.stdout);
    let stderr = String::from_utf8_lossy(&outcome.stderr);
    assert!(
        outcome.status.success(),
        "the staged run of `{CHILD_TEST}` failed, which means the authority did NOT prefer the \
         target dir it was running out of.\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    // A filter that matched nothing also exits 0, so the child must be seen to have RUN.
    assert!(
        stdout.contains("1 passed"),
        "the staged run must actually execute `{CHILD_TEST}` - an empty filter match exits 0 and \
         would make this proof vacuous.\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
}
