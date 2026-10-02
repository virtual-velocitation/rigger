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
//!      product) are UNCONDITIONAL for every real target the product ships on - never
//!      `optional = true`, so no feature flag is needed to activate them - whether that
//!      declaration lives in plain `[dependencies]` or (spec 93 criterion 1, THE CORE
//!      LANE IS PURE) `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`, the
//!      target-cfg table that excludes ONLY the wasm32-unknown-unknown target this
//!      product never shipped the adapter on and that cannot compile it; and
//!   4. the hand-maintained blueprint `docs/architecture.md` no longer describes
//!      `kurrentdb` as a build-time cargo feature (a broken install command or a
//!      self-contradictory "behind the feature" line).
//!
//! Deliberately NOT feature-gated: it parses a text file and touches no backend
//! symbol, so it runs identically in both feature lanes.

mod common;

use common::repo::repo_text;
use common::repo::table_declares_key;
use common::repo::table_lines;

#[path = "common/retired_feature.rs"]
mod retired_feature;
use retired_feature::assert_no_src_line_gates_on;

/// The declaration line for dependency `dep` in `[header]`, if present.
fn dependency_line(manifest: &str, header: &str, dep: &str) -> Option<String> {
    table_lines(manifest, header).into_iter().find(|line| {
        let t = line.trim();
        t.starts_with(&format!("{dep} "))
            || t.starts_with(&format!("{dep}="))
            || t.starts_with(&format!("{dep}."))
    })
}

/// The target-cfg table (spec 93 criterion 1) that carries every dependency unconditional
/// on every real target this product ships on, but absent for the wasm32-unknown-unknown
/// target the pure `core` lane also builds for - `rusqlite`, `kurrentdb`, `tokio`, `rustix`,
/// `fs2`, `uuid` and `ignore` all live here now instead of in `[dependencies]` because none
/// of them compile for that one target, not because any of them became optional.
const NATIVE_TARGET_HEADER: &str = "target.'cfg(not(target_arch = \"wasm32\"))'.dependencies";

/// `dep`'s declaration line, checked across BOTH homes an unconditional-on-native
/// dependency may legitimately live in: plain `[dependencies]`, or the native-only
/// target-cfg table `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`.
fn native_dependency_line(manifest: &str, dep: &str) -> Option<String> {
    dependency_line(manifest, "dependencies", dep)
        .or_else(|| dependency_line(manifest, NATIVE_TARGET_HEADER, dep))
}

/// DEP HYGIENE (spec 47, criterion 2, part one): the `kurrentdb` cargo feature is
/// retired. With no `[features]` entry named `kurrentdb`, cargo rejects `-F
/// kurrentdb` as an unknown feature, and no `#[cfg(feature = "kurrentdb")]` in the
/// tree can ever compile the adapter conditionally again.
#[test]
fn kurrentdb_cargo_feature_is_retired() {
    let m = repo_text("Cargo.toml");
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
    let m = repo_text("Cargo.toml");
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
    let m = repo_text("Cargo.toml");
    for dep in ["kurrentdb", "tokio"] {
        let line = native_dependency_line(&m, dep).unwrap_or_else(|| {
            panic!(
                "{dep} must be an unconditional dependency on every real (non-wasm32) target \
                 (spec 47) - either [dependencies] or \
                 [target.'cfg(not(target_arch = \"wasm32\"))'.dependencies] (spec 93 criterion 1)"
            )
        });
        assert!(
            !line.contains("optional = true"),
            "{dep} must be unconditional (not optional): the adapter and its runtime are part of \
             the product, compiled into every real build; got: {line}"
        );
    }
}

/// LIBRARY-CONSUMER EDGE (spec 47): retiring the cargo feature un-gates
/// `pub mod kurrentdb` (`src/eventstore/mod.rs`), so a downstream crate that embeds the
/// harness "imports the same modules from the rigger crate directly" (architecture) -
/// no `-F kurrentdb` recompile. This ungated test crate IS such a consumer. Naming the
/// fully-qualified path `rigger::eventstore::kurrentdb::Store::open` forces the compiler
/// to resolve the adapter as a PUBLIC, unconditionally-compiled module: if a regression
/// re-gated it behind `#[cfg(feature = ...)]`, this would fail to COMPILE in the
/// `--no-default-features` lane, so its compilation is the guarantee. The runtime check
/// only proves the resolved symbol is a real function pointer (never null); opening a
/// real backend needs a server, so it is not called here.
///
/// This is the LIBRARY edge, distinct from the binary's internal `open_store` unit
/// test: an internal caller says nothing about the module's visibility to an external
/// crate, which is exactly the embed-the-harness promise spec 47 makes.
#[test]
fn kurrentdb_adapter_is_a_public_library_symbol_in_every_lane() {
    // The type annotation coerces the zero-sized fn item to a fn pointer; the `_`
    // infers the adapter's error type without naming it, so this stays robust to that
    // type's visibility.
    let open: fn(&str) -> Result<rigger::eventstore::kurrentdb::Store, _> =
        rigger::eventstore::kurrentdb::Store::open;
    assert!(
        open as usize != 0,
        "the adapter's `open` constructor must resolve as a public library symbol (spec 47)"
    );
}

/// SOURCE HYGIENE (spec 47): retiring the cargo feature is only half the story - NO
/// `#[cfg(feature = "kurrentdb")]` (nor `cfg!(feature = "kurrentdb")`) may remain
/// anywhere under `src/`. A lingering gate would be WORSE than a dead flag: a
/// `cfg(feature = "...")` predicate on a feature that no longer exists always evaluates
/// FALSE, so it would silently compile the guarded code OUT of every build - the adapter
/// (or any code it still gated) would vanish with no error and no failing build. The
/// Cargo.toml guard proves the feature is undeclared; this proves no source still
/// branches on it. Space-insensitive so it catches `feature="kurrentdb"` too.
#[test]
fn no_source_still_gates_on_the_retired_kurrentdb_feature() {
    assert_no_src_line_gates_on(
        &["cfg(feature=\"kurrentdb\"", "cfg!(feature=\"kurrentdb\""],
        "the `kurrentdb` cargo feature is retired (spec 47), so no source may still gate on it - \
         a `cfg(feature = \"kurrentdb\")` predicate on a now-undefined feature evaluates FALSE and \
         silently compiles the guarded code out of every build.",
    );
}

/// Whether a single line of `docs/architecture.md` references `kurrentdb` in a cargo
/// FEATURE position - the retired build-time flag - rather than as the runtime backend.
/// It normalizes the line (backticks removed, lowercased) and matches only feature-shaped
/// forms, so it flags a broken/self-contradictory reference while leaving the legitimate
/// runtime selector `--eventstore kurrentdb` and the unrelated `-F turbovec` feature alone.
fn line_references_retired_kurrentdb_feature(raw: &str) -> bool {
    use std::sync::OnceLock;
    // A `kurrentdb` token inside a cargo feature LIST (an install/build/test command or a
    // Cargo.toml `features:` note): `--features ...,kurrentdb`, `features: ...,kurrentdb`,
    // `-F kurrentdb`. Between the marker and the token only feature-list characters may
    // appear, so unrelated prose cannot bridge the two.
    static LIST: OnceLock<regex::Regex> = OnceLock::new();
    // A cfg/toml gate on the feature: `feature = "kurrentdb"`.
    static CFG: OnceLock<regex::Regex> = OnceLock::new();
    // Prose that calls the adapter a cargo feature: `kurrentdb feature`,
    // `kurrentdb (feature)`, `kurrentdb cargo feature`. The optional `(` / `cargo`
    // between the two words keeps `--eventstore kurrentdb (no ... no cargo feature)` OUT
    // (there "kurrentdb" is not immediately followed by a `feature` token).
    static PROSE: OnceLock<regex::Regex> = OnceLock::new();
    let list = LIST.get_or_init(|| {
        regex::Regex::new(r"(?:--features|features:|-f)[a-z0-9_,\-\s]*kurrentdb").unwrap()
    });
    let cfg = CFG.get_or_init(|| regex::Regex::new(r#"feature\s*=\s*"kurrentdb""#).unwrap());
    let prose =
        PROSE.get_or_init(|| regex::Regex::new(r"kurrentdb\s*\(?\s*(?:cargo\s+)?feature").unwrap());

    let normalized = raw.replace('`', "").to_lowercase();
    list.is_match(&normalized) || cfg.is_match(&normalized) || prose.is_match(&normalized)
}

/// DOC BLUEPRINT HYGIENE (spec 47): the hand-maintained blueprint `docs/architecture.md`
/// must not describe `kurrentdb` as a build-time cargo feature once the flag is retired.
/// Two stale forms are more than cosmetic:
///   1. an install/build command that ACTIVATES a `kurrentdb` cargo feature
///      (`cargo install ... --features ...,kurrentdb`, `-F kurrentdb`, `cargo test
///      --features kurrentdb`) is a BROKEN, executable command: Cargo.toml no longer
///      declares that feature, so cargo rejects it ("package `rigger` does not contain
///      this feature: kurrentdb"). A consumer who copy-pastes it hits a hard failure -
///      the exact default-build consumer spec 47 exists to serve; and
///   2. prose that calls the adapter a `kurrentdb` cargo feature ("behind the `kurrentdb`
///      feature", "kurrentdb (feature)", "features: ...,kurrentdb") directly contradicts
///      the retirement the same file now states ("compiled into every build ... no cargo
///      feature"), leaving the blueprint self-contradictory.
///
/// This is the periphery guard for the boundary the blueprint itself is: a doc-only
/// regression that re-introduces any feature-shaped `kurrentdb` reference fails HERE at
/// `cargo test` time instead of shipping a broken install command. It deliberately leaves
/// the legitimate runtime selector `--eventstore kurrentdb` and the unrelated `-F turbovec`
/// feature untouched - only a `kurrentdb` reference in a cargo-FEATURE position is an
/// offender. Like the rest of this file it reads a text file and touches no backend symbol,
/// so it runs identically in both feature lanes.
#[test]
fn architecture_blueprint_has_no_retired_kurrentdb_feature_reference() {
    let text = repo_text("docs/architecture.md");

    let offenders: Vec<String> = text
        .lines()
        .enumerate()
        .filter(|(_, line)| line_references_retired_kurrentdb_feature(line))
        .map(|(idx, line)| format!("{}: {}", idx + 1, line.trim()))
        .collect();

    assert!(
        offenders.is_empty(),
        "the `kurrentdb` cargo feature is retired (spec 47), so the blueprint \
         docs/architecture.md must not reference it in a feature position - an install \
         command activating it (`--features ...,kurrentdb`) is a BROKEN command cargo now \
         rejects, and calling the adapter a `kurrentdb` feature contradicts the same file's \
         `compiled into every build` statement. Offending lines: {offenders:#?}"
    );
}
