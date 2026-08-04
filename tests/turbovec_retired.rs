//! Spec 57 - Retire turbovec: the DEPENDENCY DIET (criterion 2).
//!
//! The vector-embedding grounder is retired: the knowledge graph is the lookup
//! surface, and the embedding sidecar was measured to add zero marginal recall while
//! taxing every build, install, and step. This criterion OWNS the packaging removal -
//! that the `turbovec` cargo feature no longer exists (so `cargo build -F turbovec` is
//! rejected by cargo as an unknown feature) and that the embedding / ONNX dependencies
//! are absent from the dependency tree of BOTH feature lanes.
//!
//! These tests read the committed `Cargo.toml` and scan `src/` (both resolved from
//! `CARGO_MANIFEST_DIR` so they are CWD-independent) and assert, structurally, that:
//!   1. the `turbovec` cargo feature no longer exists, and no other feature's value
//!      re-wires it - so cargo rejects `-F turbovec` as an unknown feature; and
//!   2. the embedding engine (`turbovec`, `fastembed`), the ONNX Runtime stack (`ort`,
//!      `ort-sys`), and the raw-flock shim (`libc`) whose sole activator was the
//!      feature are absent from `[dependencies]` - so neither lane's dependency tree
//!      carries them; and
//!   3. `symbols` (the structural grounder) joins the DEFAULT feature set - the default
//!      build gets LIGHTER, not different; and
//!   4. no `#[cfg(feature = "turbovec")]` gate survives anywhere under `src/` - a gate
//!      on a now-undeclared feature always evaluates FALSE and would silently compile
//!      the guarded code out of every build.
//!
//! The dependency-tree claim is proven STRUCTURALLY against the manifest (as spec 47's
//! kurrentdb retirement proved its own): an absent optional dependency cannot be pulled
//! into EITHER lane, so there is nothing to enumerate with `cargo tree`. Deliberately
//! NOT feature-gated: it parses text files and touches no grounder symbol, so it runs
//! identically in both feature lanes and is a real member of each lane's test battery.

use std::path::{Path, PathBuf};

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
/// `key.<sub> = ...`, or a bare `key`)? Continuation lines of a multi-line array value
/// (e.g. `    "dep:foo",`) never match, so this keys on the DECLARATION line.
fn table_declares_key(manifest: &str, header: &str, key: &str) -> bool {
    table_lines(manifest, header).iter().any(|line| {
        let t = line.trim();
        t == key
            || t.starts_with(&format!("{key} "))
            || t.starts_with(&format!("{key}="))
            || t.starts_with(&format!("{key}."))
    })
}

/// Recurse `dir`, invoking `visit(path, file_text)` for every `.rs` file beneath it.
/// A std-only walk (no extra dev-dependency) sufficient for scanning the crate's `src/`.
fn for_each_rs_file(dir: &Path, visit: &mut dyn FnMut(&Path, &str)) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => panic!("cannot read dir {}: {e}", dir.display()),
    };
    for entry in entries {
        let path = entry.expect("dir entry must be readable").path();
        if path.is_dir() {
            for_each_rs_file(&path, visit);
        } else if path.extension().is_some_and(|x| x == "rs") {
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            visit(&path, &text);
        }
    }
}

/// DEP DIET (spec 57, criterion 2, part one): the `turbovec` cargo feature is retired.
/// With no `[features]` entry named `turbovec`, cargo rejects `-F turbovec` as an unknown
/// feature, and no `#[cfg(feature = "turbovec")]` in the tree can ever compile the
/// embedding grounder conditionally again. No surviving feature's VALUE may re-wire the
/// embedding stack either (a `dep:turbovec` / `dep:fastembed` / `dep:ort` activation):
/// the whole feature is retired, root and branch.
#[test]
fn turbovec_cargo_feature_is_retired() {
    let m = manifest_text();
    assert!(
        !table_declares_key(&m, "features", "turbovec"),
        "the `turbovec` cargo feature must be gone from [features] so `cargo build -F turbovec` \
         is rejected as an unknown feature (spec 57); Cargo.toml still declares it"
    );
    // No feature's value may re-activate the embedding stack.
    let features_body = table_lines(&m, "features").join("\n");
    for token in ["turbovec", "fastembed", "dep:ort", "ort-sys"] {
        assert!(
            !features_body.contains(token),
            "no [features] entry may reference `{token}` after turbovec is retired (spec 57); \
             got:\n{features_body}"
        );
    }
}

/// DEP DIET (spec 57, criterion 2, part two): every dependency that existed only to serve
/// the embedding grounder is gone from `[dependencies]`, so NEITHER feature lane's
/// dependency tree carries it. `turbovec` and `fastembed` are the embedding engine;
/// `ort` and `ort-sys` are the ONNX Runtime stack; `libc` was the raw-flock shim whose
/// sole activator was the retired feature. An absent optional dependency cannot be pulled
/// into either lane, so this manifest assertion IS the both-lanes dependency-tree proof.
#[test]
fn embedding_and_onnx_dependencies_are_absent_from_both_lanes() {
    let m = manifest_text();
    for dep in ["turbovec", "fastembed", "ort", "ort-sys", "libc"] {
        assert!(
            !table_declares_key(&m, "dependencies", dep),
            "`{dep}` existed only to serve the retired embedding grounder and must be absent from \
             [dependencies] (spec 57), so neither feature lane's dependency tree carries it; \
             Cargo.toml still declares it"
        );
    }
}

/// DEP DIET (spec 57, criterion 2, part three): retiring turbovec makes the default build
/// LIGHTER, not different - the structural `symbols` grounder REMAINS in the default
/// feature set (it was already there alongside turbovec; turbovec simply leaves). A plain
/// `cargo build` therefore still ships structural grounding; `--no-default-features` stays
/// the deliberate light opt-out.
#[test]
fn symbols_remains_in_the_default_feature_set() {
    let m = manifest_text();
    let default_line = table_lines(&m, "features")
        .into_iter()
        .find(|l| l.trim_start().starts_with("default"))
        .expect("[features] must declare a `default` set");
    assert!(
        default_line.contains("symbols"),
        "the `symbols` structural grounder must stay in the default feature set so a plain \
         `cargo build` still ships structural grounding (spec 57); got: {default_line}"
    );
    assert!(
        !default_line.contains("turbovec"),
        "the retired `turbovec` feature must NOT be in the default set (spec 57); got: {default_line}"
    );
}

/// SOURCE HYGIENE (spec 57): retiring the cargo feature is only half the story - NO
/// `#[cfg(feature = "turbovec")]` (nor `cfg!(feature = "turbovec")`, nor the `not(...)`
/// form) may remain anywhere under `src/`. A lingering gate would be WORSE than a dead
/// flag: a `cfg(feature = "...")` predicate on a feature that no longer exists always
/// resolves deterministically (the positive form to FALSE, the `not` form to TRUE), so it
/// would silently compile the guarded code into or out of every build with no error. The
/// manifest guard proves the feature is undeclared; this proves no source still branches
/// on it. Space-insensitive so it catches `feature="turbovec"` too.
#[test]
fn no_source_still_gates_on_the_retired_turbovec_feature() {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    for_each_rs_file(&src, &mut |path, text| {
        for (idx, line) in text.lines().enumerate() {
            let squeezed: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            if squeezed.contains("feature=\"turbovec\"") {
                offenders.push(format!("{}:{}", path.display(), idx + 1));
            }
        }
    });
    assert!(
        offenders.is_empty(),
        "the `turbovec` cargo feature is retired (spec 57), so no source may still gate on it - \
         a `cfg(feature = \"turbovec\")` predicate on a now-undefined feature resolves \
         deterministically and silently compiles the guarded code in or out of every build. \
         Offending lines: {offenders:?}"
    );
}
