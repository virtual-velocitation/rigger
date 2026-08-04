//! Spec 57 - Retire turbovec: the DEPENDENCY DIET (criterion 2), proven at the TOOL
//! BOUNDARY by driving the real dependency resolver.
//!
//! A sibling test (`turbovec_retired.rs`) proves the diet STRUCTURALLY: it reads the
//! committed `Cargo.toml` text and asserts the `turbovec` cargo feature is undeclared and
//! the embedding / ONNX dependency lines are gone. That is the inside-out, static half.
//! It cannot, by construction, prove what the resolver actually DOES, because it never
//! runs it. Two claims in the criterion are behavioral and live one layer out from the
//! manifest text:
//!
//!   1. "building with `-F turbovec` is REJECTED by cargo" - a statement about the
//!      resolver's behavior when handed the retired feature name, not about a line in a
//!      file. A manifest that merely lacks a `[features].turbovec` entry does not, on its
//!      own, prove the resolver rejects the name: an optional dependency of the same name
//!      would synthesize an implicit feature, and only asking the resolver settles it.
//!   2. "the embedding / ONNX dependencies are absent from the dependency TREE of both
//!      lanes" - the tree is the TRANSITIVE closure, so a crate the manifest never names
//!      directly could still be pulled in by some other dependency. Only the resolved tree
//!      shows what actually ships; the manifest shows only the direct edges.
//!
//! So this file completes the criterion from the OUTSIDE: it invokes the resolver
//! (`cargo tree`, a resolve-only command that never compiles the crate, so it is fast and
//! hermetic under `--offline`) and asserts the retired feature is rejected while a
//! surviving grounder feature is accepted, and that the resolved tree of EACH lane is free
//! of every retired embedding / ONNX crate. Together with the static half, the diet is
//! proven both in the manifest and in the resolver's actual output.
//!
//! Deliberately NOT feature-gated: it spawns a subprocess and touches no grounder symbol,
//! so it compiles and runs identically in the default and the `--no-default-features`
//! lanes, and is a real member of each lane's test battery.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::{Command, Output};

/// Crate names that existed ONLY to serve the retired embedding grounder: the embedding
/// engine (`turbovec`, `fastembed`) and the ONNX Runtime stack (`ort`, `ort-sys`). After
/// the diet none of these may appear in the resolved dependency tree of either lane.
const RETIRED_CRATES: &[&str] = &["turbovec", "fastembed", "ort", "ort-sys"];

/// Run `cargo <args>` with the crate root as the working directory so the resolver reads
/// THIS crate's committed manifest regardless of the process CWD (integration tests may run
/// from anywhere). The cargo binary is the one that built this test - `env!("CARGO")` is set
/// by cargo at build time and always points at a real cargo - so the test needs nothing on
/// `PATH`. `--offline` keeps it hermetic: the resolver reads the warm local cache the build
/// already populated and never reaches the network.
fn cargo(args: &[&str]) -> Output {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let cargo_bin = env!("CARGO");
    Command::new(cargo_bin)
        .current_dir(Path::new(manifest_dir))
        .arg("tree")
        .arg("--offline")
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn `{cargo_bin} tree {args:?}`: {e}"))
}

/// The set of crate names in a `cargo tree` listing. Each line is optional tree-drawing
/// glyphs (`│ ├ └ ─` and spaces) followed by `<name> v<version> ...`, so the crate name is
/// the first whitespace-delimited token AFTER the leading glyphs are stripped. Keying on
/// the exact token (not a substring) is what keeps `ort` from matching unrelated crates
/// like `report` / `sort` / `export`.
fn crate_names(tree_stdout: &str) -> BTreeSet<String> {
    tree_stdout
        .lines()
        .filter_map(|line| {
            let name = line
                .trim_start_matches(|c: char| {
                    c.is_whitespace() || matches!(c, '│' | '├' | '└' | '─')
                })
                .split_whitespace()
                .next()?;
            (!name.is_empty()).then(|| name.to_string())
        })
        .collect()
}

/// DEP DIET (spec 57, criterion 2): the resolver REJECTS `-F turbovec` as an unknown
/// feature, while a surviving grounder feature (`-F symbols`) still resolves. The retired
/// name failing is the headline of the criterion; the surviving name passing is the
/// DISCRIMINATING control - it proves the resolver works in this environment, so the
/// turbovec failure is specifically the feature's retirement and not a broken toolchain or
/// a cold cache masquerading as a rejection.
#[test]
fn cargo_rejects_the_retired_turbovec_feature_and_accepts_a_surviving_one() {
    // The retired feature: cargo must reject it as unknown, before any compilation.
    let retired = cargo(&["--edges", "no-dev", "--features", "turbovec"]);
    assert!(
        !retired.status.success(),
        "`cargo tree -F turbovec` must FAIL: the `turbovec` feature is retired (spec 57), so \
         the resolver must reject the name. It exited successfully instead.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&retired.stdout),
        String::from_utf8_lossy(&retired.stderr),
    );
    let stderr = String::from_utf8_lossy(&retired.stderr).to_lowercase();
    assert!(
        stderr.contains("turbovec") && stderr.contains("feature"),
        "the rejection must name the missing `turbovec` feature (so the failure is the \
         retirement, not some unrelated resolver error); cargo's stderr was:\n{stderr}"
    );

    // The discriminating control: a grounder feature that SURVIVED the retirement resolves
    // cleanly, proving the resolver and the offline cache are healthy here.
    let surviving = cargo(&["--edges", "no-dev", "--features", "symbols"]);
    assert!(
        surviving.status.success(),
        "`cargo tree -F symbols` must SUCCEED: `symbols` is a surviving grounder feature, so \
         its rejection would mean the resolver itself is broken here (not a turbovec signal).\n\
         stderr:\n{}",
        String::from_utf8_lossy(&surviving.stderr),
    );
}

/// DEP DIET (spec 57, criterion 2): the resolved dependency TREE of BOTH feature lanes is
/// free of every retired embedding / ONNX crate. This is the transitive-closure counterpart
/// to the manifest scan: a retired crate could reappear as a TRANSITIVE dependency of some
/// surviving crate without any direct manifest edge naming it, and only the resolved tree
/// would show it. Both lanes are checked because the criterion promises the diet for each:
/// the default (`symbols`) build and the `--no-default-features` (grep-only) build.
#[test]
fn no_retired_embedding_or_onnx_crate_survives_in_either_lane_tree() {
    for (lane, extra) in [
        ("default", &["--edges", "no-dev"][..]),
        (
            "no-default-features",
            &["--edges", "no-dev", "--no-default-features"][..],
        ),
    ] {
        let out = cargo(extra);
        assert!(
            out.status.success(),
            "`cargo tree` must resolve the {lane} lane so its tree can be inspected; it failed.\n\
             stderr:\n{}",
            String::from_utf8_lossy(&out.stderr),
        );
        let names = crate_names(&String::from_utf8_lossy(&out.stdout));
        // Guard against a vacuous pass: a real crate tree has many nodes, so an empty or
        // near-empty parse (e.g. a format change that broke `crate_names`) fails LOUD here
        // rather than silently satisfying the disjointness check below.
        assert!(
            names.len() > 10,
            "the {lane} lane must resolve a non-trivial dependency tree (got {} crate names); \
             a near-empty parse would make the absence check vacuous",
            names.len(),
        );
        let survivors: Vec<&str> = RETIRED_CRATES
            .iter()
            .copied()
            .filter(|c| names.contains(*c))
            .collect();
        assert!(
            survivors.is_empty(),
            "the {lane} lane's dependency tree still carries retired embedding / ONNX crate(s) \
             {survivors:?} after the diet (spec 57); they must be absent from every lane's tree"
        );
    }
}
