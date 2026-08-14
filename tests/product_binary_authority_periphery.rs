//! Periphery (integration) test for the ONE product-binary authority every suite now calls:
//! `common::rigger_bin`.
//!
//! WHAT THE INSIDE-OUT TESTS ARE STRUCTURALLY BLIND TO. `tests/product_binary_location.rs` pins
//! the pure derivation (`common::product_binary_from` over a path), pins that the derivation
//! declines outside a `deps/` dir, pins that the resolved binary exists in the target dir this
//! process runs from, and pins that no other suite bakes the compile-time path. None of those can
//! see the two properties the authority is actually built around, because both are properties of a
//! CHOICE BETWEEN CANDIDATES that only one process can observe: its RESOLUTION ORDER, and the
//! condition on that order.
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
//! ORDER, proved by making the two candidates BOTH EXIST AND DIFFER - the copied/restored target
//! dir, which is the case the order exists for - and observing which one the authority returns.
//!
//! THE CONDITION ON THAT ORDER, proved by making the derived candidate NOT EXIST. The preference is
//! documented to hold only for a derived path that is really there; a resolver that took the
//! derived answer unconditionally, or that never reached the compiled-beside fallback, hands every
//! caller a path nobody built, and all sixteen suites that spawn it die with the bare
//! `Os { code: 2, kind: NotFound }` this authority exists to eliminate - without one assertion in
//! the tree changing colour, because under an ordinary run the derived path always exists.
//!
//! Both observations work the only way an external consumer can: they stage a second target dir
//! beside the real one, link this very test executable into it (and, for the order proof, the
//! product too), and re-run one ignored test of this same suite from there, so the resolution runs
//! in a process whose `current_exe()` really is the moved one. Nothing is faked and nothing is
//! injected.
//!
//! Cost: the staging is hard links (a fallback copy only if the link is refused), and the whole
//! probe lives inside the target dir it is probing, so it never leaves that tree and never lands
//! on another filesystem. It compiles unconditionally, so it runs in both feature lanes.

mod common;

use std::path::{Path, PathBuf};

/// How the parent hands the child the one answer it must produce. An environment variable, not an
/// argument, because the child is a libtest binary: its argv belongs to the harness.
const EXPECTED_PRODUCT_ENV: &str = "RIGGER_PERIPHERY_EXPECTED_PRODUCT";

/// How the parent hands the child the product path its staged target dir does NOT hold - the one
/// answer the authority may never give back.
const ABSENT_PRODUCT_ENV: &str = "RIGGER_PERIPHERY_ABSENT_PRODUCT";

/// The name of the child test, spelled once so the spawn and the definition cannot drift.
const CHILD_TEST: &str = "the_resolved_product_is_the_one_beside_the_running_executable";

/// The name of the fallback child test, spelled once for the same reason.
const FALLBACK_CHILD_TEST: &str = "the_resolver_refuses_a_derived_product_that_is_not_there";

/// The opening of the authority's documented refusal, which it raises when NEITHER candidate
/// exists. Matched as text on purpose: it is the difference between "there was nothing to resolve"
/// and "there was something and the authority did not take it", and an external consumer has no
/// other way to tell those apart. If the wording moves, this reddens loudly rather than quietly
/// reclassifying a defect as an empty environment.
const REFUSAL: &str = "no `rigger` binary to drive:";

/// The label the refusal puts in front of the compiled-beside candidate it names.
const REFUSAL_FALLBACK_LABEL: &str = "compiled-beside ";

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

/// This test executable and the `<target>/<profile>` dir it is running out of.
///
/// Both observations stage a target dir AROUND the running suite, so both need that suite to be a
/// cargo-run integration binary sitting in a `deps/` dir. A runner that stages test executables
/// somewhere else is told so HERE, in one sentence naming the path, rather than through a confusing
/// failure three steps later in a child process.
fn running_suite() -> (PathBuf, PathBuf) {
    let exe = std::env::current_exe().expect("the running test executable");
    let deps = exe.parent().expect("<target>/<profile>/deps").to_path_buf();
    assert_eq!(
        deps.file_name().and_then(|n| n.to_str()),
        Some("deps"),
        "this proof stages a target dir around the running suite, so the suite must be running \
         from a cargo `deps/` dir; it is at {}",
        exe.display()
    );
    let profile = deps.parent().expect("<target>/<profile>").to_path_buf();
    (exe, profile)
}

/// This very suite, re-staged in a target dir of its OWN, so a child process runs with a
/// `current_exe()` that really is somewhere else.
struct StagedSuite {
    /// The probe root, removed once the observation is over.
    root: PathBuf,
    /// `<probe>/<profile>` - where the staged suite's DERIVED product would sit.
    profile: PathBuf,
    /// `<probe>/<profile>/deps/<suite>` - the staged executable to run.
    exe: PathBuf,
}

/// Stage this suite into a fresh target dir INSIDE the one it is running out of.
///
/// Inside, so the hard links never cross a filesystem and the probe never escapes the tree cargo
/// already owns; `tag` and the process id keep two observations (and two concurrent runs) from
/// colliding on the same probe root.
fn stage_this_suite(tag: &str, exe: &Path, profile: &Path) -> StagedSuite {
    let target = profile.parent().expect("<target>");
    let root = target.join(format!("periphery-{tag}-target-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let staged_profile = root.join(profile.file_name().expect("<profile>"));
    let staged_deps = staged_profile.join("deps");
    std::fs::create_dir_all(&staged_deps)
        .unwrap_or_else(|e| panic!("could not stage {}: {e}", staged_deps.display()));
    let staged_exe = staged_deps.join(exe.file_name().expect("the suite's file name"));
    stage(exe, &staged_exe);
    StagedSuite {
        root,
        profile: staged_profile,
        exe: staged_exe,
    }
}

/// Run one `#[ignore]`d child of this suite from `staged`, with `env` handed in, and return
/// `(status ok, stdout, stderr)`. The probe root is removed BEFORE the caller judges the outcome,
/// so a failing observation still leaves the target dir exactly as it found it.
fn run_staged_child(
    staged: &StagedSuite,
    test: &str,
    env: (&str, &Path),
) -> (bool, String, String) {
    let outcome = std::process::Command::new(&staged.exe)
        .args(["--exact", "--ignored", "--nocapture", test])
        .env(env.0, env.1)
        .output();
    let _ = std::fs::remove_dir_all(&staged.root);
    let outcome = outcome.unwrap_or_else(|e| {
        panic!(
            "could not run the staged copy of this suite at {}: {e}",
            staged.exe.display()
        )
    });
    (
        outcome.status.success(),
        String::from_utf8_lossy(&outcome.stdout).into_owned(),
        String::from_utf8_lossy(&outcome.stderr).into_owned(),
    )
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
    let (exe, profile) = running_suite();

    let baked = profile.join(product_file_name());
    assert!(
        baked.exists(),
        "the compiled-beside product must exist for this to be a proof of ORDER: with only one \
         candidate present, a resolver that consults them in either order answers the same. \
         Expected it at {}",
        baked.display()
    );

    let staged = stage_this_suite("staged", &exe, &profile);
    let staged_product = staged.profile.join(product_file_name());
    stage(&baked, &staged_product);

    assert_ne!(
        staged_product, baked,
        "the two candidates must be different paths for the order to be observable"
    );

    let (ok, stdout, stderr) =
        run_staged_child(&staged, CHILD_TEST, (EXPECTED_PRODUCT_ENV, &staged_product));
    assert!(
        ok,
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

/// THE SECOND CHILD. Driven only as a subprocess, from a staged target dir that holds NO product,
/// with the path that dir does not hold handed in through the environment.
///
/// `#[ignore]`d for the same reason as the first: in the parent's own process a product really does
/// sit beside the executable, so there would be nothing to fall back FROM.
#[test]
#[ignore = "driven as a subprocess from a product-less staged target dir by the_authority_falls_back_rather_than_hand_back_a_path_nobody_built"]
fn the_resolver_refuses_a_derived_product_that_is_not_there() {
    let absent = PathBuf::from(std::env::var(ABSENT_PRODUCT_ENV).unwrap_or_else(|_| {
        panic!(
            "{ABSENT_PRODUCT_ENV} is not set: this test is driven as a subprocess by \
             the_authority_falls_back_rather_than_hand_back_a_path_nobody_built, which stages a \
             target dir holding no product and hands in the path that dir does not hold. It \
             asserts nothing on its own"
        )
    }));
    assert!(
        !absent.exists(),
        "the staged target dir must hold NO product for this to prove anything, but {} is there",
        absent.display()
    );

    let resolved = common::rigger_bin();
    assert_ne!(
        resolved, absent,
        "the authority handed back the derived path even though nothing was built there. Every \
         suite that spawns it then dies with a bare NotFound naming no cause - which is the exact \
         failure this authority exists to eliminate, and the preference for the derived candidate \
         is documented to hold only while that candidate EXISTS"
    );
    assert!(
        resolved.exists(),
        "the authority must resolve a product that is really there; it answered {}",
        resolved.display()
    );
}

/// THE CONDITION ON THE ORDER, observed from outside with the derived candidate ABSENT.
///
/// The staging reproduces the case the `exists` guard is written for: this suite's executable is
/// linked into a target dir of its own, and NO product is staged beside it. The derived candidate
/// therefore names a path nobody built, and the only answer that can be spawned is the
/// compiled-beside one.
///
/// WHY THE EXISTING PROOFS CANNOT SEE THIS. Under an ordinary `cargo test` the derived path always
/// exists, so a resolver that dropped the `exists` guard - or one that never reached the
/// compiled-beside fallback at all - returns exactly the same answer as a correct one and leaves
/// every assertion in the tree green. Both were measured: with either change in place, this file's
/// order proof and the whole of `tests/product_binary_location.rs` still report 6 passed / 0
/// failed.
///
/// HOW A MISSING FALLBACK IS TOLD APART FROM A REFUSED ONE, because these are opposite verdicts on
/// the same exit code. When neither candidate exists the authority is DOCUMENTED to refuse, naming
/// both candidates - and in an environment whose compiled-beside product is genuinely gone (a
/// target dir that was moved rather than copied) that refusal is correct behavior, not a defect. So
/// a failing child is not read as a verdict on its own: the refusal is parsed, and the fallback it
/// names is checked. A refusal that names a fallback which EXISTS is the defect; a refusal that
/// names one which does not is the contract being honored in an environment that has nothing to
/// fall back to, and it still pins the refusal's shape.
#[test]
fn the_authority_falls_back_rather_than_hand_back_a_path_nobody_built() {
    let (exe, profile) = running_suite();
    let staged = stage_this_suite("fallback", &exe, &profile);

    let absent_product = staged.profile.join(product_file_name());
    assert!(
        !absent_product.exists(),
        "the staged target dir must hold no product for this to be a proof of the FALLBACK; \
         something is at {}",
        absent_product.display()
    );

    let (ok, stdout, stderr) = run_staged_child(
        &staged,
        FALLBACK_CHILD_TEST,
        (ABSENT_PRODUCT_ENV, &absent_product),
    );
    if ok {
        // A filter that matched nothing also exits 0, so the child must be seen to have RUN.
        assert!(
            stdout.contains("1 passed"),
            "the staged run must actually execute `{FALLBACK_CHILD_TEST}` - an empty filter match \
             exits 0 and would make this proof vacuous.\n--- stdout ---\n{stdout}\n--- stderr \
             ---\n{stderr}"
        );
        return;
    }

    let both = format!("{stdout}\n{stderr}");
    let refusal = both.find(REFUSAL).unwrap_or_else(|| {
        panic!(
            "the staged run of `{FALLBACK_CHILD_TEST}` failed WITHOUT the authority's documented \
             refusal, so the authority answered - and answered with a product that is not there. \
             That is the derived candidate winning unconditionally.\n--- stdout ---\n{stdout}\n\
             --- stderr ---\n{stderr}"
        )
    });
    assert!(
        both[refusal..].contains(&absent_product.display().to_string()),
        "the refusal must name the derived candidate it could not use ({}), so an operator reading \
         it learns which target dir was searched.\n--- stdout ---\n{stdout}\n--- stderr \
         ---\n{stderr}",
        absent_product.display()
    );

    let named = both[refusal..]
        .split_once(REFUSAL_FALLBACK_LABEL)
        .and_then(|(_, rest)| rest.split('\n').next())
        .map(str::trim)
        .unwrap_or_else(|| {
            panic!(
                "the refusal must name the compiled-beside candidate as well, so the reader can \
                 tell an empty target dir from a refused fallback.\n--- stdout ---\n{stdout}\n\
                 --- stderr ---\n{stderr}"
            )
        });
    assert!(
        !Path::new(named).exists(),
        "the authority REFUSED to resolve a product while {named} was sitting there to fall back \
         to. The compiled-beside candidate is the documented second choice precisely for a target \
         dir the derived answer is missing from.\n--- stdout ---\n{stdout}\n--- stderr \
         ---\n{stderr}"
    );
}
