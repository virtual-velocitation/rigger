//! Build script: embed a build-provenance identifier so the compiled binary can report
//! WHICH source it was built from.
//!
//! An agent cannot otherwise tell whether an installed `rigger` binary matches the source,
//! which is what makes the workflow-drift warning ambiguous. We resolve a git commit/describe
//! id at build time and hand it to the compiler as `RIGGER_BUILD_PROVENANCE`, so `main.rs`
//! (and the drift diagnostic that consumes it) can read it with `env!`. The value is always
//! non-empty: outside a git checkout (e.g. a build from a published tarball with no `.git`) it
//! falls back to a sentinel rather than failing the build.

use std::path::Path;
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

fn main() {
    // Re-run when the build script itself changes.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build/gitsemver.rs");
    println!("cargo:rerun-if-changed=build/watch.rs");
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
