//! Spec 74: version derivation is DELEGATED to the `go-gitsemver` binary
//! (github.com/MyCarrier-DevOps/go-gitsemver, GitVersion-compatible) - rigger never
//! reimplements the bump algorithm, only invokes the tool and folds its output into the
//! string the binary embeds. This file has no `fn main` and is included by `#[path]`
//! from BOTH `build.rs` (compiled into the build-script crate, invoked at COMPILE time)
//! and `tests/gitsemver_derivation.rs` (compiled into that integration-test binary,
//! driven against fixture git repositories and the real binary). That is the ONE
//! derivation seam: whatever the test proves is exactly the code the shipped binary
//! runs, never a parallel reimplementation of it.
//!
//! The reported version is `go-gitsemver`'s `FullSemVer` for the built commit, with
//! `ShortSha` folded into its build-metadata segment. Semver ignores build metadata for
//! ordering, so this adds exact-commit identity on top of the derived order without
//! disturbing it - the Problem spec 74 opens with ("only the build hash - identity
//! without order - tells two binaries apart") stays solved even as the reported version
//! now also carries real, comparable order.
//!
//! Falls back to the bare crate semver plus the explicit [`UNVERSIONED_SUFFIX`] marker
//! whenever `go-gitsemver` cannot produce a `FullSemVer` for ANY reason - the binary
//! missing from PATH, the source not a git checkout, a non-zero exit, or empty output.
//! One uniform "could not derive" signal regardless of cause, mirroring criterion 2's
//! validate advisory (which folds every `+unversioned` cause into one message for the
//! same reason: the embedded marker itself cannot distinguish its cause). The build
//! must NEVER fail because the derivation tool could not run.

use std::path::Path;
use std::process::Command;

/// Appended to the bare crate semver whenever the real derivation is unavailable, so a
/// reader can never mistake the fallback for a genuinely derived version.
pub const UNVERSIONED_SUFFIX: &str = "+unversioned";

/// Run `<bin> -p <dir> --show-variable <variable>`, trimmed. `None` on ANY failure: the
/// binary missing from PATH (`Command::output` errors with `NotFound`), `dir` not a git
/// checkout, a non-zero exit, or empty output.
fn show_variable(bin: &str, dir: &Path, variable: &str) -> Option<String> {
    let out = Command::new(bin)
        .arg("-p")
        .arg(dir)
        .arg("--show-variable")
        .arg(variable)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let value = String::from_utf8(out.stdout).ok()?.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

/// Folds `short_sha` into `full_semver`'s build-metadata segment: a new dot-separated
/// identifier when metadata already exists (go-gitsemver's own commits-since-tag
/// count), else the sole `+` metadata.
fn append_build_metadata(full_semver: &str, short_sha: &str) -> String {
    if full_semver.contains('+') {
        format!("{full_semver}.{short_sha}")
    } else {
        format!("{full_semver}+{short_sha}")
    }
}

/// The version rigger reports: `go-gitsemver`'s `FullSemVer` for the commit checked out
/// at `dir`, under whatever `go-gitsemver.yml` `dir` has committed, with `ShortSha`
/// folded in as build metadata. Falls back to the crate's own semver
/// (`CARGO_PKG_VERSION`, resolved for whichever crate includes this file - build.rs and
/// this test binary are both compiled as part of the SAME package, so the value is
/// identical either way) plus [`UNVERSIONED_SUFFIX`] whenever `go-gitsemver` cannot
/// produce a `FullSemVer`. `bin` is the executable name/path to invoke (parameterized
/// so tests can force a not-found tool without touching PATH).
pub fn derive_version(bin: &str, dir: &Path) -> String {
    let Some(full_semver) = show_variable(bin, dir, "FullSemVer") else {
        return format!("{}{UNVERSIONED_SUFFIX}", env!("CARGO_PKG_VERSION"));
    };
    match show_variable(bin, dir, "ShortSha") {
        Some(short_sha) => append_build_metadata(&full_semver, &short_sha),
        None => full_semver,
    }
}
