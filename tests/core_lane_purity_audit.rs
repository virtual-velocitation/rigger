//! Spec 93 criterion 1, THE CORE LANE IS PURE: `--no-default-features --features core` builds
//! the library natively and for `wasm32-unknown-unknown`, an audit over the sources finds no
//! `core` module importing the banned set, and `default` still carries `store` so today's two
//! lanes are byte-for-byte unchanged in behavior.
//!
//! `docs/audit/core-lane-purity.json` is the evidence this file checks: the exact set of
//! always-on (`core`) module files derived from `src/lib.rs`'s own `#[cfg(...)]` gates, the
//! banned-pattern set THE PURITY RULE names, and every occurrence the same scan this file runs
//! actually found when the record was authored - each one cited by file:line and paired with
//! the nearby `#[cfg(any(feature = "store", not(feature = "core")))]` line that excludes it.
//!
//! THE SPLIT THIS FILE MAKES, deliberately (mirrors `tests/compiler_pass_stage1_audit.rs`,
//! spec 87): the checks below that are cheap (pure Rust, read files already on disk, no
//! subprocess, no recompile) run on every `cargo test`, always, and are the PRIMARY, always-
//! enforced guard - they re-run the identical textual scan that produced the record and fail if
//! the tree has drifted from it in either direction (a new occurrence, or a stale citation). The
//! one check that is not cheap - it actually builds the library for `wasm32-unknown-unknown`
//! plus a native `--features core` build and test pass - is gated behind
//! `RIGGER_CORE_LANE_VERIFY=1`, the same opt-in shape `RIGGER_COMPILER_PASS_VERIFY=1` (spec 87)
//! and `RIGGER_AUDIT_WRITE=1` already use: a real, permanent per-`cargo test` cost (a wasm cross-
//! compile plus a from-scratch native recompile under a feature combination no other test
//! builds) is not paid by every future unit's gate cycle to reprove a fact the cheap tests
//! already guard against regressing. The expensive check was run by hand while this unit was
//! authored and confirmed clean - see this file's own `verification` field in the JSON and this
//! unit's own `DecisionMade` record.

use std::fs;
use std::path::Path;
use std::process::Command;

#[path = "common/audit_record.rs"]
mod audit_record;
use audit_record::read_audit_record;

/// The committed core-lane purity record this suite checks against the real tree.
const PURITY_RECORD: &str = "docs/audit/core-lane-purity.json";

/// A line, 1-indexed, from a source file.
fn nth_line(path: &str, line: u64) -> String {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {path}: {e}"));
    text.lines()
        .nth((line - 1) as usize)
        .unwrap_or_else(|| panic!("{path}:{line} - file has fewer lines than that"))
        .to_string()
}

/// The banned-pattern set THE PURITY RULE (spec 93 Design) names, split into the std-namespace
/// paths (never compiler-enforced - `std::fs`/`std::process`/`std::net` all still TYPE CHECK for
/// `wasm32-unknown-unknown`, they just fail at runtime, so a wasm build alone cannot catch a
/// regression here) and the banned crate names (compiler-enforced by the wasm build: none of
/// them are in the `core`-only dependency graph for that target, so a rogue `use` simply fails to
/// resolve there - see the expensive check below). Crate patterns require a `::` or a leading
/// `use ` so a string-literal test fixture (e.g. `eventstore::mod.rs`'s `redact_conn` tests,
/// which build fixture strings like `"kurrentdb://user:pass@host"`) is never misread as a real
/// import.
const STD_PATTERNS: &[&str] = &["std::fs", "std::process", "std::net", "SystemTime::now"];
const CRATE_PATTERNS: &[&str] = &[
    "rusqlite",
    "tokio::",
    "use tokio",
    "rustix::",
    "use rustix",
    "fs2::",
    "use fs2",
    "ignore::",
    "use ignore",
    "uuid::",
    "use uuid",
    "kurrentdb::",
    "use kurrentdb",
];

/// Every banned-pattern occurrence in `path`'s source text, skipping comment lines (`//...`,
/// including `///`/`//!` doc comments - so a comment EXPLAINING an absence, e.g. "SystemTime::now
/// is itself a banned import", is never misread as a real usage). Returns `(line, pattern)`
/// pairs, 1-indexed, in file order.
fn scan(path: &str) -> Vec<(usize, &'static str)> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {path}: {e}"));
    let mut hits = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("//") {
            continue;
        }
        for p in STD_PATTERNS.iter().chain(CRATE_PATTERNS.iter()) {
            if line.contains(p) {
                hits.push((i + 1, *p));
            }
        }
    }
    hits
}

#[test]
fn purity_audit_record_has_the_shape_every_consumer_relies_on() {
    let r = read_audit_record(PURITY_RECORD);
    let modules = r["core_modules"]
        .as_array()
        .expect("core_modules must be a JSON array");
    assert!(!modules.is_empty(), "core_modules must not be empty");
    for (i, m) in modules.iter().enumerate() {
        let path = m
            .as_str()
            .unwrap_or_else(|| panic!("core_modules[{i}] must be a string"));
        assert!(
            Path::new(path).is_file(),
            "core_modules[{i}] names {path}, which does not exist in the tree"
        );
    }

    let found = r["found_occurrences"]
        .as_array()
        .expect("found_occurrences must be a JSON array");
    for (i, entry) in found.iter().enumerate() {
        for field in ["file", "pattern", "reason"] {
            assert!(
                entry.get(field).and_then(|v| v.as_str()).is_some(),
                "found_occurrences[{i}] is missing a string `{field}`"
            );
        }
        for field in ["line", "gated_by_line"] {
            assert!(
                entry.get(field).and_then(|v| v.as_u64()).is_some(),
                "found_occurrences[{i}] is missing a numeric `{field}`"
            );
        }
    }
}

/// Every file the record names as `core_modules` must genuinely carry no un-gated
/// `#[cfg(any(feature = "store", not(feature = "core")))]`-EXCLUDED module declaration for
/// itself in `src/lib.rs` - i.e. the record's own module list has not drifted from `lib.rs`'s
/// actual gates. A cheap structural cross-check, not a full parse: every listed file's basename
/// (or `mod.rs` parent dir name) must appear as a `pub mod <name>;` / `mod <name>;` line in
/// `lib.rs` (or, for a submodule under `contextgraph`/`eventstore`/`console`, in that module's
/// own `mod.rs`) that is NOT immediately preceded by the store-gate attribute.
#[test]
fn every_listed_core_module_is_declared_ungated_in_its_parent() {
    let r = read_audit_record(PURITY_RECORD);
    let modules: Vec<String> = r["core_modules"]
        .as_array()
        .expect("core_modules must be a JSON array")
        .iter()
        .map(|v| {
            v.as_str()
                .expect("core_modules entries must be strings")
                .to_string()
        })
        .collect();

    let is_declared_ungated = |parent_text: &str, name: &str| -> bool {
        let lines: Vec<&str> = parent_text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let t = line.trim();
            if t == format!("pub mod {name};") || t == format!("mod {name};") {
                // Walk upward over any doc-comment lines to the nearest non-doc-comment
                // predecessor; a gate, if present, is that line.
                let mut j = i;
                while j > 0 && lines[j - 1].trim_start().starts_with("///") {
                    j -= 1;
                }
                let gated = j > 0
                    && lines[j - 1].contains("#[cfg(")
                    && lines[j - 1].contains("feature = \"store\"");
                return !gated;
            }
        }
        false
    };

    let lib_text = fs::read_to_string("src/lib.rs").expect("reading src/lib.rs");
    let contextgraph_text =
        fs::read_to_string("src/contextgraph/mod.rs").expect("reading src/contextgraph/mod.rs");
    let eventstore_text =
        fs::read_to_string("src/eventstore/mod.rs").expect("reading src/eventstore/mod.rs");
    let console_text =
        fs::read_to_string("src/console/mod.rs").expect("reading src/console/mod.rs");
    let domain_lib_text = fs::read_to_string("crates/rigger-domain/src/lib.rs")
        .expect("reading crates/rigger-domain/src/lib.rs");

    for m in &modules {
        let (parent_text, name): (&str, &str) = if m == "src/contextgraph/mod.rs" {
            (&lib_text, "contextgraph")
        } else if m == "src/eventstore/mod.rs" {
            (&lib_text, "eventstore")
        } else if m == "src/console/mod.rs" {
            (&lib_text, "console")
        } else if let Some(rest) = m.strip_prefix("src/contextgraph/") {
            (&contextgraph_text, rest.trim_end_matches(".rs"))
        } else if let Some(rest) = m.strip_prefix("src/eventstore/") {
            (&eventstore_text, rest.trim_end_matches(".rs"))
        } else if let Some(rest) = m.strip_prefix("src/console/") {
            (&console_text, rest.trim_end_matches(".rs"))
        } else if let Some(rest) = m.strip_prefix("crates/rigger-domain/src/") {
            (&domain_lib_text, rest.trim_end_matches(".rs"))
        } else {
            let name = m
                .strip_prefix("src/")
                .and_then(|s| s.strip_suffix(".rs"))
                .unwrap_or_else(|| panic!("core_modules entry {m} is not under src/"));
            (&lib_text, name)
        };
        assert!(
            is_declared_ungated(parent_text, name),
            "{m} (declared as `{name}`) is not an ungated module declaration in its parent - \
             the purity-audit record has drifted from src/lib.rs's actual feature gates"
        );
    }
}

/// The primary, always-run enforcement: re-scan every `core_modules` file for the
/// `banned_patterns` and assert the result matches `found_occurrences` EXACTLY - zero more
/// (a genuine new violation), zero fewer (a stale, no-longer-true citation) - and that each
/// cited occurrence's `gated_by_line` genuinely carries the required
/// `#[cfg(any(feature = "store", not(feature = "core")))]` text, so a future edit that widens
/// (or removes) that gate while leaving the banned call in place is caught here rather than
/// only at the next hand-run wasm build.
#[test]
fn no_core_module_imports_the_banned_set_outside_its_cited_store_gate() {
    let r = read_audit_record(PURITY_RECORD);
    let modules: Vec<String> = r["core_modules"]
        .as_array()
        .expect("core_modules must be a JSON array")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    let expected: Vec<(String, u64, String)> = r["found_occurrences"]
        .as_array()
        .expect("found_occurrences must be a JSON array")
        .iter()
        .map(|e| {
            (
                e["file"].as_str().unwrap().to_string(),
                e["line"].as_u64().unwrap(),
                e["pattern"].as_str().unwrap().to_string(),
            )
        })
        .collect();

    let mut actual: Vec<(String, u64, String)> = Vec::new();
    for m in &modules {
        for (line, pattern) in scan(m) {
            actual.push((m.clone(), line as u64, pattern.to_string()));
        }
    }
    actual.sort();
    let mut expected_sorted = expected.clone();
    expected_sorted.sort();
    assert_eq!(
        actual, expected_sorted,
        "the live banned-pattern scan over core_modules no longer matches \
         docs/audit/core-lane-purity.json's found_occurrences - either a NEW banned import \
         landed in a core module (THE PURITY RULE violation, fix it or gate it and re-run \
         RIGGER_AUDIT_WRITE=1-equivalent by hand to update the record), or a previously-gated \
         one was removed (update the record to drop the stale citation)"
    );

    // Every cited occurrence's gated_by_line genuinely carries the required attribute text.
    let found = r["found_occurrences"].as_array().unwrap();
    for entry in found {
        let file = entry["file"].as_str().unwrap();
        let gated_by_line = entry["gated_by_line"].as_u64().unwrap();
        let text = nth_line(file, gated_by_line);
        assert!(
            text.contains("#[cfg(") && text.contains("feature = \"store\""),
            "{file}:{gated_by_line} was recorded as the store-gate for a banned-pattern \
             occurrence but reads:\n  {text}\nwhich does not carry the required \
             #[cfg(any(feature = \"store\", not(feature = \"core\")))] text"
        );
    }
}

/// The expensive ground-truth check this unit's own `DecisionMade` cites as having been run by
/// hand: re-derive a clean native build, a clean `wasm32-unknown-unknown` build (the compiler-
/// enforced proof that every banned CRATE is genuinely unreachable, since none of them are in
/// the core-only dependency graph for that target), and a clean native test pass, all on the
/// `--no-default-features --features core` lane, scoped to `--lib` (the binary is intentionally
/// store-only - spec 93 Design's THE FEATURE SPLIT names "the binary" as one of the things
/// `store` gates - so `--lib` is the correct, not a narrowed, scope here). Skipped (not failed)
/// unless `RIGGER_CORE_LANE_VERIFY=1` is set.
#[test]
fn rigger_core_lane_verify_rederives_a_clean_native_and_wasm_build() {
    if std::env::var("RIGGER_CORE_LANE_VERIFY").as_deref() != Ok("1") {
        eprintln!(
            "skipping rigger_core_lane_verify_rederives_a_clean_native_and_wasm_build: set \
             RIGGER_CORE_LANE_VERIFY=1 to actually re-run the native build, the \
             wasm32-unknown-unknown build and the native test pass this checks (see this \
             file's own module doc comment for why it is opt-in)"
        );
        return;
    }

    let run = |args: &[&str], label: &str| {
        let out = Command::new("cargo")
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("spawning cargo {args:?}: {e}"));
        assert!(
            out.status.success(),
            "{label} was NOT clean, contradicting docs/audit/core-lane-purity.json's \
             verification.result:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    };

    run(
        &[
            "build",
            "--lib",
            "--no-default-features",
            "--features",
            "core",
        ],
        "native core-lane build",
    );
    run(
        &[
            "build",
            "--lib",
            "--no-default-features",
            "--features",
            "core",
            "--target",
            "wasm32-unknown-unknown",
        ],
        "wasm32-unknown-unknown core-lane build (the target must be installed: \
         `rustup target add wasm32-unknown-unknown`)",
    );
    run(
        &[
            "test",
            "--lib",
            "--no-default-features",
            "--features",
            "core",
        ],
        "native core-lane test pass",
    );
}
