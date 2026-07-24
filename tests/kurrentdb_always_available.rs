//! Spec 47 - KurrentDB is always available: packaging invariants.
//!
//! The shared-store capability (many users' rigger instances appending to one
//! KurrentDB so agent context is shared across a team) is a first-class product
//! capability. A consumer who installs the DEFAULT build must be able to point at a
//! shared store with a runtime flag, never a recompile - so the adapter compiles
//! into every build and its old build-time cargo feature is retired.
//!
//! These tests read the committed `Cargo.toml` (resolved from `CARGO_MANIFEST_DIR`
//! so they are CWD-independent) and assert, structurally, that:
//!   1. the `kurrentdb` cargo feature no longer exists - so `cargo build -F
//!      kurrentdb` is rejected by cargo as an unknown feature; and
//!   2. `testcontainers`, which drives the contract TEST only, is a dev-dependency
//!      and never sits in the production dependency tree; and
//!   3. `kurrentdb` and `tokio` (the gRPC client and its runtime, part of the
//!      product) are UNCONDITIONAL `[dependencies]`, not optional.
//!
//! Deliberately NOT feature-gated: it parses a text file and touches no backend
//! symbol, so it runs identically in both feature lanes.

use std::path::PathBuf;

/// The committed crate manifest, resolved from the manifest dir so the test does not
/// depend on the process CWD (integration tests may run from anywhere).
fn manifest_text() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read Cargo.toml at {}: {e}", path.display()))
}

/// The body lines of the first top-level `[header]` table: every line after the
/// `[header]` line up to (not including) the next line that opens a new `[...]`
/// table. Empty when the table is absent.
fn table_lines(manifest: &str, header: &str) -> Vec<String> {
    let want = format!("[{header}]");
    let mut in_table = false;
    let mut out = Vec::new();
    for raw in manifest.lines() {
        let line = raw.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_table = line == want;
            continue;
        }
        if in_table {
            out.push(raw.to_string());
        }
    }
    out
}

/// Does the `[header]` table declare a key named `key` at its top level (`key = ...`,
/// `key.<sub> = ...`, or a bare `key`)? Continuation lines of a multi-line array
/// value (e.g. `    "dep:foo",`) never match, so this keys on the DECLARATION line.
fn table_declares_key(manifest: &str, header: &str, key: &str) -> bool {
    table_lines(manifest, header).iter().any(|line| {
        let t = line.trim();
        t == key
            || t.starts_with(&format!("{key} "))
            || t.starts_with(&format!("{key}="))
            || t.starts_with(&format!("{key}."))
    })
}

/// The declaration line for dependency `dep` in `[header]`, if present.
fn dependency_line(manifest: &str, header: &str, dep: &str) -> Option<String> {
    table_lines(manifest, header).into_iter().find(|line| {
        let t = line.trim();
        t.starts_with(&format!("{dep} "))
            || t.starts_with(&format!("{dep}="))
            || t.starts_with(&format!("{dep}."))
    })
}

/// DEP HYGIENE (spec 47, criterion 2, part one): the `kurrentdb` cargo feature is
/// retired. With no `[features]` entry named `kurrentdb`, cargo rejects `-F
/// kurrentdb` as an unknown feature, and no `#[cfg(feature = "kurrentdb")]` in the
/// tree can ever compile the adapter conditionally again.
#[test]
fn kurrentdb_cargo_feature_is_retired() {
    let m = manifest_text();
    assert!(
        !table_declares_key(&m, "features", "kurrentdb"),
        "the `kurrentdb` cargo feature must be gone from [features] so `cargo build -F kurrentdb` \
         is rejected as an unknown feature (spec 47); Cargo.toml still declares it"
    );
    // No feature's VALUE may wire kurrentdb in either (e.g. a `dep:kurrentdb` /
    // `dep:testcontainers` activation): the whole feature is retired.
    let features_body = table_lines(&m, "features").join("\n");
    assert!(
        !features_body.contains("kurrentdb"),
        "no [features] entry may reference kurrentdb after the flag is retired; got:\n{features_body}"
    );
}

/// DEP HYGIENE (spec 47, criterion 2, part two): `testcontainers` drives the contract
/// TEST only, so it must be a `[dev-dependencies]` entry and must NEVER appear in
/// `[dependencies]` (the production dependency tree).
#[test]
fn testcontainers_is_a_dev_dependency_only() {
    let m = manifest_text();
    assert!(
        table_declares_key(&m, "dev-dependencies", "testcontainers"),
        "testcontainers drives the contract TEST, so it must be declared under [dev-dependencies]"
    );
    assert!(
        !table_declares_key(&m, "dependencies", "testcontainers"),
        "testcontainers must NOT be in [dependencies] - it must never sit in the production \
         dependency tree (spec 47)"
    );
}

/// ALWAYS AVAILABLE (spec 47): the gRPC client `kurrentdb` and its `tokio` runtime
/// are part of the product, so they are UNCONDITIONAL `[dependencies]` - not
/// `optional = true`, which (with no feature to activate them) would leave the
/// adapter uncompilable. This guards the packaging change against a regression that
/// re-optionalizes them.
#[test]
fn kurrentdb_and_tokio_are_unconditional_dependencies() {
    let m = manifest_text();
    for dep in ["kurrentdb", "tokio"] {
        let line = dependency_line(&m, "dependencies", dep).unwrap_or_else(|| {
            panic!("{dep} must be an unconditional [dependencies] entry (spec 47)")
        });
        assert!(
            !line.contains("optional = true"),
            "{dep} must be unconditional (not optional): the adapter and its runtime are part of \
             the product, compiled into every build; got: {line}"
        );
    }
}
