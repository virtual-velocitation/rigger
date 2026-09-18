//! Periphery for spec 93, criterion 3 ("THE BUILD EMBEDS IT"): `build.rs`'s nested
//! `wasm32-unknown-unknown` cross-compile of `crates/console-core`, and its embedding into
//! the served `/console/core.wasm` route.
//!
//! `build/console_wasm.rs` is `#[path]`-included by both `build.rs` (compiled into the
//! build-script crate) and this file (compiled into this integration-test binary), the same
//! technique `tests/gitsemver_derivation.rs` and `tests/build_watch_paths.rs` already use for
//! the identical reason: a build script cannot be exercised by `cargo test` directly.
//!
//! Every check in THIS file is pure Rust over `console_wasm`'s functions - no subprocess,
//! no cross-compile - and runs on every `cargo test`, always. The one check that is not
//! cheap - independently re-cross-compiling `console-core` for `wasm32-unknown-unknown` a
//! second time and comparing it byte-for-byte against the artifact `build.rs` already
//! embedded - needs the embedded `CONSOLE_CORE_WASM` constant `src/dash.rs` deliberately
//! keeps private (a `pub` accessor with no real production consumer would itself be a
//! dead-code candidate spec 87's audit must disposition), so it lives in `src/dash.rs`'s
//! own `#[cfg(test)] mod tests` instead, gated behind `RIGGER_CONSOLE_WASM_EMBED_VERIFY=1`,
//! the same opt-in shape `RIGGER_CORE_LANE_VERIFY=1` (`tests/core_lane_purity_audit.rs`) and
//! `RIGGER_CONSOLE_CORE_ABI_VERIFY=1` (`crates/console-core/tests/exports.rs`) already use
//! for an identical real, permanent per-`cargo test` cross-compile cost.

#[path = "../build/console_wasm.rs"]
#[allow(dead_code)]
mod console_wasm;

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
    let path = PathBuf::from(CARGO_MANIFEST_DIR).join("docs/audit/core-lane-purity.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
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
