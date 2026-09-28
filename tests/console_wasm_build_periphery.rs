//! Periphery for spec 93, criterion 3 ("THE BUILD EMBEDS IT"): `build.rs`'s nested
//! `wasm32-unknown-unknown` cross-compile of `crates/console-core`, and its embedding into
//! the served `/console/core.wasm` route.
//!
//! `crates/rigger-dash/build/console_wasm.rs` is `#[path]`-included by both `build.rs` (compiled into the
//! build-script crate) and this file (compiled into this integration-test binary), the same
//! technique `tests/gitsemver_derivation.rs` and `tests/build_watch_paths.rs` already use for
//! the identical reason: a build script cannot be exercised by `cargo test` directly.
//!
//! Every check in THIS file is pure Rust over `console_wasm`'s functions - no subprocess,
//! no cross-compile - and runs on every `cargo test`, always. The one check that is not
//! cheap - independently re-cross-compiling `console-core` for `wasm32-unknown-unknown` a
//! second time and comparing it byte-for-byte against the artifact `build.rs` already
//! embedded - needs the embedded `CONSOLE_CORE_WASM` constant `crates/rigger-dash/src/dash.rs` deliberately
//! keeps private (a `pub` accessor with no real production consumer would itself be a
//! dead-code candidate spec 87's audit must disposition), so it lives in `crates/rigger-dash/src/dash.rs`'s
//! own `#[cfg(test)] mod tests` instead, gated behind `RIGGER_CONSOLE_WASM_EMBED_VERIFY=1`,
//! the same opt-in shape `RIGGER_CORE_LANE_VERIFY=1` (`tests/core_lane_purity_audit.rs`) and
//! `RIGGER_CONSOLE_CORE_ABI_VERIFY=1` (`crates/console-core/tests/exports.rs`) already use
//! for an identical real, permanent per-`cargo test` cross-compile cost.
//!
//! SDET addition: `build_wasm_artifact` itself - the one nested-cargo-invocation seam,
//! and a NEW `pub fn` this unit adds - had NO always-on coverage before this: only the
//! opt-in, expensive test above ever calls it, so a `cargo test` run without
//! `RIGGER_CONSOLE_WASM_EMBED_VERIFY=1` (every CI run, and every gate in this loop)
//! exercised none of its real-subprocess contract at all. That is exactly the boundary a
//! real infinite-recursion bug was found in while landing this unit (`d-u93c3-cargo-
//! feature-env-leak-fix`): `Command::new(cargo)` inherits this TEST PROCESS's own
//! environment by default, and Cargo only ADDS `CARGO_FEATURE_<NAME>=1` for a package's
//! active features without ever clearing an inactive one - so a naive implementation would
//! leak this test binary's own `CARGO_FEATURE_STORE`/`CARGO_FEATURE_TURBOVEC` straight
//! into the spawned command. The tests below drive `build_wasm_artifact` for real, against
//! a fixture standing in for `cargo` (`tests/fixtures/recording-cargo.sh`) that records its
//! own argv and environment - never a real (slow) cross-compile - so this always runs, on
//! every `cargo test`, and proves the actual OS-process boundary: the exact command line,
//! that RUSTFLAGS/RUSTC_WRAPPER/RUSTC_WORKSPACE_WRAPPER never leak an ambient value
//! through, that EVERY inherited `CARGO_FEATURE_*` is scrubbed, the success return path's
//! artifact path, and that a nonzero exit returns `Err` rather than panicking.

mod common;

#[path = "../crates/rigger-dash/build/console_wasm.rs"]
#[allow(dead_code)]
mod console_wasm;

use common::env_test_lock;
use common::repo::repo_text;
use std::path::PathBuf;

const CARGO_MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

// ---------------------------------------------------------------------------
// missing_target_message: cheap, always-on.
// ---------------------------------------------------------------------------

#[test]
fn missing_target_message_names_the_exact_install_command() {
    let msg = console_wasm::missing_target_message("wasm32-unknown-unknown");
    assert!(
        msg.contains("rustup target add wasm32-unknown-unknown"),
        "message must name the exact command verbatim: {msg}"
    );
}

#[test]
fn missing_target_message_never_silently_skips() {
    // Design's own wording: "it never skips the module silently" - the message must at
    // least say the module/build FAILED, not merely suggest an optional step.
    let msg = console_wasm::missing_target_message("wasm32-unknown-unknown");
    assert!(
        msg.to_lowercase().contains("failed") || msg.to_lowercase().contains("not installed"),
        "message must name the failure, not just hint at an optional install: {msg}"
    );
}

// ---------------------------------------------------------------------------
// target_installed: cheap, always-on, fixture-driven (no real toolchain needed).
// ---------------------------------------------------------------------------

#[test]
fn target_installed_true_when_the_sysroot_lib_dir_exists() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = "wasm32-unknown-unknown";
    std::fs::create_dir_all(dir.path().join("lib/rustlib").join(target).join("lib"))
        .expect("create fixture sysroot dir");
    assert!(console_wasm::target_installed(dir.path(), target));
}

#[test]
fn target_installed_false_when_the_sysroot_lib_dir_is_absent() {
    let dir = tempfile::tempdir().expect("tempdir");
    // A sysroot that has OTHER targets installed, but not this one.
    std::fs::create_dir_all(dir.path().join("lib/rustlib/x86_64-unknown-linux-gnu/lib"))
        .expect("create fixture sysroot dir");
    assert!(!console_wasm::target_installed(
        dir.path(),
        "wasm32-unknown-unknown"
    ));
}

#[test]
fn target_installed_false_on_a_nonexistent_sysroot() {
    assert!(!console_wasm::target_installed(
        std::path::Path::new("/definitely/not/a/real/sysroot/path"),
        "wasm32-unknown-unknown"
    ));
}

// ---------------------------------------------------------------------------
// core_module_paths: cheap, always-on.
// ---------------------------------------------------------------------------

#[test]
fn core_module_paths_extracts_a_flat_string_array() {
    let json =
        r#"{"core_modules": ["src/a.rs", "src/b.rs", "src/contextgraph/mod.rs"], "other": []}"#;
    assert_eq!(
        console_wasm::core_module_paths(json),
        vec!["src/a.rs", "src/b.rs", "src/contextgraph/mod.rs"]
    );
}

#[test]
fn core_module_paths_is_empty_without_a_core_modules_key() {
    assert!(console_wasm::core_module_paths(r#"{"other": [1, 2]}"#).is_empty());
}

#[test]
fn core_module_paths_reads_the_real_committed_audit_record() {
    // The genuine source of truth this seam depends on: prove the parser actually agrees
    // with `tests/core_lane_purity_audit.rs`'s own reading of the SAME file, not a
    // reimplementation drifting from it.
    let text = repo_text("docs/audit/core-lane-purity.json");
    let paths = console_wasm::core_module_paths(&text);
    assert!(
        !paths.is_empty(),
        "docs/audit/core-lane-purity.json's core_modules must not parse empty"
    );
    for p in &paths {
        assert!(
            PathBuf::from(CARGO_MANIFEST_DIR).join(p).is_file(),
            "core_modules entry {p} does not exist in the tree"
        );
    }
}

// ---------------------------------------------------------------------------
// build_wasm_artifact: the one nested-cargo-invocation seam, exercised for real against a
// recording fixture standing in for `cargo` - never a real cross-compile - so this runs on
// every `cargo test`, always. Every test here mutates real process env (the exact leak
// surface this function exists to close) so all of them share one lock, the same
// discipline `tests/build_env_authority_periphery.rs`'s `env_test_lock` uses and documents:
// `cargo test` runs a binary's tests as concurrent threads by default, and a concurrent
// env read racing a concurrent env write is a genuine POSIX getenv/setenv hazard regardless
// of which keys either side touches.
// ---------------------------------------------------------------------------

fn recording_cargo_fixture() -> PathBuf {
    PathBuf::from(CARGO_MANIFEST_DIR).join("tests/fixtures/recording-cargo.sh")
}

/// Every env var name this file's tests ever set on the real process, removed - the
/// deterministic baseline each test starts and ends on, regardless of which of them ran
/// (or panicked) before it, matching `stage_fake_sccache_on_path`'s sibling discipline in
/// `tests/build_env_authority_periphery.rs`.
fn clear_recording_cargo_env() {
    for key in [
        "RECORDING_CARGO_DUMP_FILE",
        "RECORDING_CARGO_EXIT_CODE",
        "CARGO_FEATURE_STORE",
        "CARGO_FEATURE_TURBOVEC",
        "RUSTFLAGS",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
    ] {
        std::env::remove_var(key);
    }
}

/// Read back `recording-cargo.sh`'s dump file: the exact argv and the exact environment a
/// REAL spawned process received, never a `Command` inspected in-process without ever
/// being run.
fn read_recording(dump_file: &std::path::Path) -> String {
    std::fs::read_to_string(dump_file)
        .unwrap_or_else(|e| panic!("reading the recording-cargo dump file {dump_file:?}: {e}"))
}

/// One real `build_wasm_artifact` call through the recording cargo fixture.
struct RecordedBuild {
    result: Result<PathBuf, String>,
    dump_file: PathBuf,
    target_dir: PathBuf,
    _scratch: tempfile::TempDir,
}

impl RecordedBuild {
    /// What the spawned process recorded ([`read_recording`]).
    fn recording(&self) -> String {
        read_recording(&self.dump_file)
    }
}

/// Calls the real `build_wasm_artifact` through [`recording_cargo_fixture`] into
/// `<scratch>/<target_dir_name>`, under [`env_test_lock`], with every one of `inherited` set
/// on THIS process - the recording env cleared before and after, so each call starts and
/// ends on the same deterministic baseline.
fn recorded_build(inherited: &[(&str, &str)], target_dir_name: &str) -> RecordedBuild {
    let _guard = env_test_lock();
    clear_recording_cargo_env();
    let scratch = tempfile::tempdir().expect("tempdir");
    let dump_file = scratch.path().join("recorded.txt");
    for (key, value) in inherited {
        std::env::set_var(key, value);
    }
    std::env::set_var("RECORDING_CARGO_DUMP_FILE", &dump_file);

    let target_dir = scratch.path().join(target_dir_name);
    let result = console_wasm::build_wasm_artifact(
        scratch.path(),
        recording_cargo_fixture().to_str().expect("utf-8 path"),
        "wasm32-unknown-unknown",
        &target_dir,
    );
    clear_recording_cargo_env();
    RecordedBuild {
        result,
        dump_file,
        target_dir,
        _scratch: scratch,
    }
}

/// THE BUG THIS FUNCTION EXISTS TO CLOSE (`d-u93c3-cargo-feature-env-leak-fix`): this test
/// binary is itself a `cargo test` artifact, built with `CARGO_FEATURE_STORE` and
/// `CARGO_FEATURE_TURBOVEC` plausibly ambient in a real build-script's environment; a naive
/// `Command::new(cargo)` inherits the CALLER's environment by default, so without an
/// explicit scrub every `CARGO_FEATURE_*` the caller happens to have set leaks straight into
/// the nested invocation. Sets them explicitly here (never relying on this process's own
/// ambient state, which a real build script may or may not have) so the assertion is
/// deterministic regardless of what feature flags built this very test binary.
#[test]
fn build_wasm_artifact_scrubs_every_inherited_cargo_feature_env_var_before_spawning() {
    let build = recorded_build(
        &[
            ("CARGO_FEATURE_STORE", "1"),
            ("CARGO_FEATURE_TURBOVEC", "1"),
        ],
        "target",
    );
    let result = &build.result;

    assert!(
        result.is_ok(),
        "a successful nested command must return Ok: {result:?}"
    );
    let recorded = build.recording();
    assert!(
        !recorded.contains("CARGO_FEATURE_STORE"),
        "CARGO_FEATURE_STORE leaked into the nested cargo invocation - the exact \
         infinite-recursion bug this function exists to close: {recorded}"
    );
    assert!(
        !recorded.contains("CARGO_FEATURE_TURBOVEC"),
        "CARGO_FEATURE_TURBOVEC leaked into the nested cargo invocation: {recorded}"
    );
}

/// `RUSTFLAGS` must never reach the nested command (an ambient value the outer build was
/// configured with must not silently retarget console-core's own cross-compile), and the
/// wrapper vars must be cleared to an EXPLICIT empty string (never merely absent) - the
/// distinction `build_wasm_artifact`'s own doc comment calls out: a config-file-set
/// `build.rustc-wrapper` is not undone by removing the env var, only by an explicit
/// empty-string override.
#[test]
fn build_wasm_artifact_clears_rustflags_and_empties_the_wrapper_vars() {
    let build = recorded_build(
        &[
            ("RUSTFLAGS", "-C target-cpu=native"),
            ("RUSTC_WRAPPER", "sccache"),
            ("RUSTC_WORKSPACE_WRAPPER", "sccache"),
        ],
        "target",
    );
    let result = &build.result;

    assert!(
        result.is_ok(),
        "a successful nested command returns Ok: {result:?}"
    );
    let recorded = build.recording();
    assert!(
        !recorded.lines().any(|l| l.starts_with("RUSTFLAGS=")),
        "RUSTFLAGS must be removed entirely from the nested invocation, not merely emptied: \
         {recorded}"
    );
    assert!(
        recorded.lines().any(|l| l == "RUSTC_WRAPPER="),
        "RUSTC_WRAPPER must be present but EXPLICITLY EMPTY (an empty-string override, \
         which alone defeats a config-file-set wrapper) rather than absent: {recorded}"
    );
    assert!(
        recorded.lines().any(|l| l == "RUSTC_WORKSPACE_WRAPPER="),
        "RUSTC_WORKSPACE_WRAPPER must be present but EXPLICITLY EMPTY: {recorded}"
    );
}

/// The exact command line `build.rs` depends on: release, locked, the right package, the
/// right target, and its OWN target-dir (never the caller's shared build cache/lock).
#[test]
fn build_wasm_artifact_invokes_the_expected_release_locked_command_line() {
    let build = recorded_build(&[], "nested-target");
    let (result, target_dir) = (&build.result, &build.target_dir);

    assert!(
        result.is_ok(),
        "a successful nested command returns Ok: {result:?}"
    );
    let recorded = build.recording();
    let args_line = recorded
        .lines()
        .find(|l| l.starts_with("ARGS:"))
        .expect("the fixture always records an ARGS line");
    let expected = format!(
        "ARGS:build --release --locked -p console-core --target wasm32-unknown-unknown \
         --target-dir {}",
        target_dir.display()
    );
    assert_eq!(
        args_line, expected,
        "build_wasm_artifact must invoke exactly this command line"
    );
}

/// The success path's return value: the release artifact's path under the caller's own
/// `target_dir`, joined `<target>/release/console_core.wasm` - the exact path `dash.rs`'s
/// `OUT_DIR` copy destination is built from in `build.rs`.
#[test]
fn build_wasm_artifact_returns_the_release_artifact_path_on_success() {
    let RecordedBuild {
        result, target_dir, ..
    } = recorded_build(&[], "nested-target");

    let artifact = result.expect("a successful nested command returns Ok");
    assert_eq!(
        artifact,
        target_dir
            .join("wasm32-unknown-unknown")
            .join("release")
            .join("console_core.wasm"),
        "the returned path must be target_dir/<target>/release/console_core.wasm"
    );
}

/// A nonzero exit from the nested command must return `Err` naming the target - never a
/// panic - so `build.rs`'s own caller can format its own diagnostic around it (as it does,
/// wrapping this in `panic!("... {e}")` itself only at the top level).
#[test]
fn build_wasm_artifact_returns_err_never_panics_on_a_nonzero_exit() {
    let result = recorded_build(&[("RECORDING_CARGO_EXIT_CODE", "1")], "nested-target").result;

    let err = result.expect_err("a nonzero exit must be reported as Err, never a panic");
    assert!(
        err.contains("console-core") && err.contains("wasm32-unknown-unknown"),
        "the error must name the package and target: {err}"
    );
}

/// A cargo binary that cannot even be spawned (an unresolvable path) must also return
/// `Err`, never panic - the OTHER real-subprocess failure mode `build_wasm_artifact`'s own
/// doc comment promises to report rather than crash on.
#[test]
fn build_wasm_artifact_returns_err_never_panics_when_cargo_cannot_be_spawned() {
    let _guard = env_test_lock();
    clear_recording_cargo_env();
    let scratch = tempfile::tempdir().expect("tempdir");
    let target_dir = scratch.path().join("nested-target");

    let result = console_wasm::build_wasm_artifact(
        scratch.path(),
        "/definitely/not/a/real/cargo/binary/rigger-u93c3-sdet-test",
        "wasm32-unknown-unknown",
        &target_dir,
    );
    clear_recording_cargo_env();

    let err = result.expect_err("an unspawnable cargo binary must be reported as Err");
    assert!(
        err.contains("spawning"),
        "the error must name the spawn failure: {err}"
    );
}
