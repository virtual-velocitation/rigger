//! Spec 93 criterion 3, THE BUILD EMBEDS IT: the pure decision logic behind `build.rs`'s
//! nested `wasm32-unknown-unknown` cross-compile of `crates/console-core`, extracted into
//! its own file so it can be proven from a test binary via the same `#[path]` inclusion
//! `build/gitsemver.rs` and `build/watch.rs` already established - a build script cannot be
//! exercised by `cargo test` directly (it is compiled and run as its own crate before the
//! package builds, never linked into a test binary).
//!
//! Three seams live here:
//!
//! - [`core_module_paths`]: derives the nested build's `cargo:rerun-if-changed` watch list
//!   from `docs/audit/core-lane-purity.json`'s own `core_modules` array (spec 93 criterion
//!   1's canonical record of every file the `core` feature compiles unconditionally) instead
//!   of a second, hand-maintained copy of that list - "the script reruns only when a `core`
//!   module or the member crate changes" (spec 93 Design, THE BUILD) means this watch must
//!   stay scoped to exactly that set, never the whole package, so a mutation sweep over an
//!   unrelated `store`-only file (e.g. `conductor.rs`) never re-triggers the cross-compile.
//!   A tiny hand-rolled extraction, not a JSON-parsing build-dependency: the file's shape is
//!   already a committed, test-guarded convention (`tests/core_lane_purity_audit.rs`), a flat
//!   array of double-quoted path strings.
//! - [`target_installed`]: whether the `wasm32-unknown-unknown` target is installed, checked
//!   BEFORE attempting the nested build at all - by looking for the target's standard-library
//!   directory under the resolved rustc sysroot, the same directory `rustup target add`
//!   populates and `rustup target remove` empties. This is toolchain-manager-agnostic and
//!   immune to rustc/cargo's own diagnostic wording changing across versions, unlike matching
//!   on a failed build's stderr text.
//! - [`missing_target_message`]: the exact failure text (spec 93 Design, THE BUILD: "the
//!   build fails with a message that names the exact command, `rustup target add
//!   wasm32-unknown-unknown`").
//!
//! [`build_wasm_artifact`] is the one nested-cargo-invocation seam itself, shared between
//! `build.rs`'s production use and an opt-in periphery test that reproduces the same command
//! independently and compares its output byte-for-byte against the embedded artifact - never
//! two independent implementations of the same subprocess command.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Parses the `"core_modules"` array out of `docs/audit/core-lane-purity.json`'s raw text.
/// Returns the listed path strings in file order, or an empty `Vec` if the key or its `[`/`]`
/// delimiters are not found - the caller decides whether that is fatal (a genuinely missing
/// or malformed audit record IS fatal for `build.rs`, since it is the sole source of the
/// watch list; a test exercising the parser alone can distinguish "empty because absent"
/// from "empty because the array itself is empty").
pub fn core_module_paths(json_text: &str) -> Vec<String> {
    let key = "\"core_modules\"";
    let after_key = match json_text.find(key) {
        Some(i) => &json_text[i + key.len()..],
        None => return Vec::new(),
    };
    let open = match after_key.find('[') {
        Some(i) => i,
        None => return Vec::new(),
    };
    let close = match after_key[open..].find(']') {
        Some(i) => i,
        None => return Vec::new(),
    };
    let body = &after_key[open + 1..open + close];
    body.split(',')
        .filter_map(|entry| {
            let entry = entry.trim();
            let entry = entry.strip_prefix('"')?;
            let entry = entry.strip_suffix('"')?;
            (!entry.is_empty()).then(|| entry.to_string())
        })
        .collect()
}

/// True when `target`'s standard-library directory exists under `sysroot` - the directory
/// `rustup target add <target>` populates and `rustup target remove <target>` empties. A
/// toolchain-manager-agnostic, version-independent signal: it works the same whether the
/// target was installed via rustup, a distro package, or a vendored sysroot, and it never
/// depends on matching rustc/cargo's own diagnostic wording (which changes across versions).
pub fn target_installed(sysroot: &Path, target: &str) -> bool {
    sysroot
        .join("lib")
        .join("rustlib")
        .join(target)
        .join("lib")
        .is_dir()
}

/// The exact failure text for a missing `target` (spec 93 Design, THE BUILD: "the build
/// fails with a message that names the exact command, `rustup target add
/// wasm32-unknown-unknown`; it never skips the module silently"). A free function so a test
/// can assert its wording without spawning rustc or cargo at all.
pub fn missing_target_message(target: &str) -> String {
    format!(
        "console-core failed to cross-compile for `{target}` (spec 93 criterion 3): the \
         target is not installed on this toolchain. Install it with:\n\n    rustup target add {target}\n\n\
         This is an operator/CI prerequisite - no rigger unit installs it for you."
    )
}

/// Cross-compile `crates/console-core` for `target` in release mode, `--locked`, into its
/// own `target_dir` (never the caller's shared build cache/lock), with the caller's own
/// `RUSTFLAGS` and rustc wrapper cleared so neither can leak into the nested compile. `cargo`
/// is the resolved cargo binary (build scripts get this via the `CARGO` env var; a plain
/// invocation passes `"cargo"`). Returns the produced `.wasm` artifact's path on success, or
/// the nested command's failure as an `Err` (never a panic, so a caller - `build.rs` or a
/// test - decides how to report it).
pub fn build_wasm_artifact(
    project_root: &Path,
    cargo: &str,
    target: &str,
    target_dir: &Path,
) -> Result<PathBuf, String> {
    let mut cmd = Command::new(cargo);
    cmd.args([
        "build",
        "--release",
        "--locked",
        "-p",
        "console-core",
        "--target",
        target,
        "--target-dir",
    ])
    .arg(target_dir)
    .current_dir(project_root)
    // Cleared, not merely removed: a config-file-set `build.rustc-wrapper` (this
    // project's own `.cargo/config.toml` sets one) is NOT undone by removing the env
    // var, since cargo would just fall back to reading the config file - only an
    // explicit empty-string override clears it (verified empirically against this
    // exact config file).
    .env("RUSTC_WRAPPER", "")
    .env("RUSTC_WORKSPACE_WRAPPER", "")
    .env_remove("RUSTFLAGS");

    // THE LEAK THIS CLOSES (found by running the real nested build, not reasoned about):
    // this function is called from INSIDE `rigger`'s own build script, a process Cargo
    // already populated with `CARGO_FEATURE_<NAME>=1` for every one of the OUTER build's
    // own active features (e.g. `CARGO_FEATURE_STORE=1` for a default build) - and,
    // unlike `OUT_DIR`/`PROFILE`/`CARGO_MANIFEST_DIR` (which Cargo unconditionally
    // OVERWRITES for every build script it spawns), `CARGO_FEATURE_*` is only ever
    // ADDED for a package's actually-active features, never explicitly cleared for an
    // inactive one. `std::process::Command` inherits the parent's environment by
    // default, so without this scrub the nested cargo process (and, transitively, ITS
    // OWN build-script invocations) inherits the outer build's `CARGO_FEATURE_STORE=1`
    // ALONGSIDE the correctly-resolved `CARGO_FEATURE_CORE=1` for `console-core`'s
    // default `core` lane - which means
    // `rigger`'s build script sees BOTH set inside the nested build too, evaluates
    // `needs_wasm` true again, and spawns ANOTHER nested build inside itself: genuine,
    // unbounded recursion (observed in practice terminating only when a `--target-dir`
    // path nested deep enough hit the OS argv-length limit). Scrubbing every inherited
    // `CARGO_FEATURE_*` name gives the nested invocation a clean slate that reflects
    // ONLY what `console-core`'s own manifest declares, regardless of which features
    // happened to be active in whatever process is calling this function.
    for (key, _) in std::env::vars() {
        if key.starts_with("CARGO_FEATURE_") {
            cmd.env_remove(key);
        }
    }

    let status = cmd
        .status()
        .map_err(|e| format!("spawning `{cargo} build ... --target {target}`: {e}"))?;
    if !status.success() {
        return Err(format!(
            "nested `{cargo} build -p console-core --target {target}` failed ({status})"
        ));
    }
    Ok(target_dir
        .join(target)
        .join("release")
        .join("console_core.wasm"))
}
