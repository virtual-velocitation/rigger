//! Build script: embed a build-provenance identifier so the compiled binary can report
//! WHICH source it was built from.
//!
//! An agent cannot otherwise tell whether an installed `rigger` binary matches the source,
//! which is what makes the workflow-drift warning ambiguous. We resolve a git commit/describe
//! id at build time and hand it to the compiler as `RIGGER_BUILD_PROVENANCE`, so `main.rs`
//! (and the drift diagnostic that consumes it) can read it with `env!`. The value is always
//! non-empty: outside a git checkout (e.g. a build from a published tarball with no `.git`) it
//! falls back to a sentinel rather than failing the build.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// Spec 74: the go-gitsemver derivation logic has no `fn main` of its own and is shared,
// unchanged, with `tests/gitsemver_derivation.rs` via the same `#[path]` inclusion - see
// `build/gitsemver.rs` for why (build scripts cannot be exercised by `cargo test`
// directly, so the derivation seam is proven from a test binary that includes the exact
// same source instead of a reimplementation of it).
#[path = "build/gitsemver.rs"]
mod gitsemver;

// Spec 74 round 4: the rerun-if-changed path list is its own module for the identical
// reason `gitsemver` above is - see `build/watch.rs`'s module doc comment.
#[path = "build/watch.rs"]
mod watch;

// Spec 93 criterion 3, THE BUILD EMBEDS IT: the nested wasm32-unknown-unknown cross-compile
// of `crates/console-core` - see `build/console_wasm.rs`'s own module doc comment for why
// this is a `#[path]`-included plain file rather than logic inlined here.
#[path = "build/console_wasm.rs"]
mod console_wasm;

/// The target `src/dash.rs` embeds and serves at `/console/core.wasm`.
const CONSOLE_WASM_TARGET: &str = "wasm32-unknown-unknown";

fn main() {
    // Re-run when the build script itself changes.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build/gitsemver.rs");
    println!("cargo:rerun-if-changed=build/watch.rs");
    println!("cargo:rerun-if-changed=build/console_wasm.rs");
    // The committed go-gitsemver config: an edit changes what FullSemVer derives to.
    println!("cargo:rerun-if-changed=go-gitsemver.yml");

    // Re-embed the id whenever the checked-out commit moves, so a rebuilt binary carries the
    // provenance of the source it was actually built from. Watching git HEAD and the ref it
    // resolves to is resolved THROUGH git, so it works both in a plain clone and in a linked
    // worktree (where `.git` is a file, not a directory). This watch ALSO covers the
    // go-gitsemver derivation below: besides HEAD/ref, it watches the loose tag-ref
    // directory and packed-refs, since go-gitsemver's Mainline mode takes the nearest
    // reachable tag as a PRIMARY input, a state the HEAD/ref watch alone cannot see - see
    // `build/watch.rs` for the full rationale.
    for path in watch::git_watch_paths(Path::new(".")) {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    let provenance = git_provenance().unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=RIGGER_BUILD_PROVENANCE={provenance}");

    // Version derivation is delegated entirely to go-gitsemver at COMPILE time only;
    // the binary never invokes the tool (or git) again at runtime.
    let version = gitsemver::derive_version("go-gitsemver", Path::new("."));
    println!("cargo:rustc-env=RIGGER_GITSEMVER_VERSION={version}");

    build_console_core_wasm();
}

/// Spec 93 criterion 3, THE BUILD EMBEDS IT: cross-compile `crates/console-core` for
/// `wasm32-unknown-unknown` and hand `src/dash.rs` a path it can `include_bytes!`.
///
/// Only runs when the outer compile actually needs the artifact: `src/dash.rs` (the sole
/// consumer) compiles under exactly `any(feature = "store", not(feature = "core"))` -
/// `src/lib.rs`'s own gate - so this mirrors that SAME predicate via the `CARGO_FEATURE_*`
/// env vars Cargo hands every build script. The pure `--no-default-features --features
/// core` lane therefore never triggers the nested cross-compile and never needs the target
/// installed (CONSTRAINTS WALK, spec 93: "Core lane natively - builds and its tests run on
/// the host; the wasm artifact is only ever produced by the nested build").
fn build_console_core_wasm() {
    let needs_wasm =
        env::var("CARGO_FEATURE_STORE").is_ok() || env::var("CARGO_FEATURE_CORE").is_err();
    if !needs_wasm {
        return;
    }

    // Rerun scoping (spec 93 Design, THE BUILD: "The script reruns only when a `core`
    // module or the member crate changes") - derived from the SAME canonical record
    // `tests/core_lane_purity_audit.rs` already treats as ground truth, never a second
    // hand-maintained list that could drift from it.
    let project_root = Path::new(".");
    let audit_json_path = project_root.join("docs/audit/core-lane-purity.json");
    println!("cargo:rerun-if-changed={}", audit_json_path.display());
    let audit_json = fs::read_to_string(&audit_json_path).unwrap_or_else(|e| {
        panic!(
            "console-core wasm embed (spec 93 criterion 3): reading {} for the core-module \
             rerun-if-changed watch list: {e}",
            audit_json_path.display()
        )
    });
    let core_modules = console_wasm::core_module_paths(&audit_json);
    assert!(
        !core_modules.is_empty(),
        "console-core wasm embed: {} parsed to an empty core_modules list - the audit record \
         is malformed or the extractor has drifted from its shape",
        audit_json_path.display()
    );
    for rel in &core_modules {
        println!("cargo:rerun-if-changed={rel}");
    }
    // The member crate's own tree (a directory watch covers Cargo.toml and every file
    // under src/ recursively, including ones added later).
    println!("cargo:rerun-if-changed=crates/console-core");

    // THE MISSING-TARGET PATH, decided (spec 93 Design): checked BEFORE attempting the
    // nested build at all, via the resolved rustc's own sysroot - never a silent skip.
    let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let sysroot = Command::new(&rustc)
        .args(["--print", "sysroot"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| PathBuf::from(s.trim().to_string()));
    if let Some(sysroot) = &sysroot {
        if !console_wasm::target_installed(sysroot, CONSOLE_WASM_TARGET) {
            panic!(
                "{}",
                console_wasm::missing_target_message(CONSOLE_WASM_TARGET)
            );
        }
    }
    // An unresolvable sysroot (rustc missing/unexpected) falls through to the nested
    // build itself, which will fail with its own diagnostic - never a silent skip of the
    // module either way.

    let out_dir =
        PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is set for every build script"));
    // Its own directory under OUT_DIR so the nested build never contends for the outer
    // build's target-dir lock (spec 93 Design, THE BUILD).
    let nested_target_dir = out_dir.join("console-core-wasm-target");
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let artifact = console_wasm::build_wasm_artifact(
        project_root,
        &cargo,
        CONSOLE_WASM_TARGET,
        &nested_target_dir,
    )
    .unwrap_or_else(|e| panic!("console-core wasm embed (spec 93 criterion 3): {e}"));

    let dest = out_dir.join("console_core.wasm");
    fs::copy(&artifact, &dest).unwrap_or_else(|e| {
        panic!(
            "console-core wasm embed: copying {} to {}: {e}",
            artifact.display(),
            dest.display()
        )
    });
}

/// `git describe --always --abbrev=12`, trimmed: a stable, commit-determined identifier that
/// prefers a reachable tag and otherwise the abbreviated commit hash. Deliberately WITHOUT
/// `--dirty` so the value is fully determined by the committed state the HEAD/ref watch tracks
/// (a `--dirty` flag would go stale unless the script also re-ran on every uncommitted edit).
/// Returns `None` when git is not runnable or the source is not a git repository.
fn git_provenance() -> Option<String> {
    let out = Command::new("git")
        .args(["describe", "--always", "--abbrev=12"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let id = String::from_utf8(out.stdout).ok()?.trim().to_string();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}
