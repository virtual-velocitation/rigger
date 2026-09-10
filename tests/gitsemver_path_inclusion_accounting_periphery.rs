//! Spec 87 criterion 1 (u87c1), SDET periphery layer: a claim-vs-tree contract test for
//! `docs/audit/stage1-compiler-pass.json`'s visibility-fix `reason` text.
//!
//! Boundary-surface accounting (mechanical probes against base `ebc9a7a`, see decision
//! `sdet-u87c1-surface-accounting`): the only real production change this unit makes is
//! `build/gitsemver.rs`'s `UNVERSIONED_SUFFIX` and `derive_version` narrowed `pub` ->
//! `pub(crate)` (the compiler's own `unreachable_pub` fix). That narrowing is safe only
//! because EVERY site that reaches these items via `#[path]` lives in the SAME crate as its
//! own inclusion (each `#[path]` attribute recompiles the source fresh, scoped to whichever
//! crate includes it) - and the record's own `reason` text turns that into a specific,
//! checkable CLAIM: "nothing outside this item's own defining crate ever reaches it at any
//! of its 3 `#[path]` inclusion sites (build.rs's own crate, src/main.rs's bin crate,
//! tests/gitsemver_derivation.rs's own test crate)".
//!
//! That claim is INCOMPLETE, found by reading the diff for the mandatory cross-module-seam
//! probe rather than trusting the record's own count: the tree has a 4th real `#[path]`
//! inclusion site, `tests/gitsemver_worktree_periphery.rs` (added under spec 74 round 3,
//! commit `6485b4a`, long before this unit's base) - it also calls
//! `gitsemver::derive_version` directly, twice. `pub(crate)` still happens to be satisfied
//! for this 4th site too (it is its own separate crate, exactly like the other two test
//! files), so this is not a functional regression by itself today - but a "compiler-proven,
//! nothing else reaches this" narrative is only as trustworthy as its own site count being
//! complete, and a future narrowing decision that trusts an undercounted list is exactly how
//! a real regression would slip through unnoticed. This file makes that count mechanically
//! self-checking instead of prose taken on faith: it scans the real tree for every genuine
//! `#[path]` inclusion of `build/gitsemver.rs` and asserts the record names each one, so
//! silently forgetting a site - in either direction, a new inclusion nobody updates the
//! record for, or a narrower rewrite left pointing at a stale list - fails loudly here.
//!
//! DELIBERATE INDEPENDENCE from `tests/compiler_pass_stage1_audit.rs` (the implementer's own
//! unit-level test, out of bounds for this SDET layer to edit): that file checks the
//! record's `file`/`line`/`name`/`to` fields are genuinely present in the tree at the
//! recorded location. This file checks a different claim - whether the record's own PROSE
//! REASON names every real `#[path]` inclusion site the tree actually has. Neither test
//! subsumes the other.
//!
//! The scan itself only counts a line as a real `#[path]` attribute when the line, trimmed of
//! leading whitespace, literally starts with `#[path` - deliberately excluding `///`/`//`
//! comment lines that merely quote the syntax as prose (this codebase has exactly such a
//! quote, `tests/code_entity_test_exclusion_periphery.rs`'s own doc comment citing this same
//! attribute as a worked example of an upward-escaping `#[path]` value; a naive substring
//! scan would misidentify that comment as a fifth inclusion site).

use std::fs;
use std::path::{Path, PathBuf};

const RECORD_PATH: &str = "docs/audit/stage1-compiler-pass.json";

/// The repo root this test binary was compiled from - never the process CWD (same convention
/// as `tests/simplification_audit.rs::repo_root`, `tests/no_os_kill_audit.rs`, and
/// `tests/duplication_catalog_contract_periphery.rs`).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every `.rs` file strictly under `dir`, recursively, appended to `out`, deterministically
/// ordered - the same walk shape as `tests/no_os_kill_audit.rs::collect_rs_files`.
fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort(); // deterministic finding order regardless of readdir order
    for path in entries {
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// A line is a real `#[path]` attribute (not a comment quoting the syntax as prose) exactly
/// when, trimmed of leading whitespace, it starts with `#[path` - a `///` or `//` comment
/// marker is always the first non-whitespace character of a comment line, so this excludes
/// every such quote while keeping every real attribute this codebase's own house style
/// writes as a standalone line.
fn is_real_path_attribute_line(line: &str) -> bool {
    line.trim_start().starts_with("#[path")
}

/// Every real `#[path = "...gitsemver.rs"]` inclusion site of `build/gitsemver.rs` found in
/// the tree, as repo-relative paths of the INCLUDING file (never the included file itself),
/// sorted and deduplicated. Scans `build.rs` (the crate-root build script), `build/` (in case
/// a sibling `#[path]`-included helper ever grows one), `src/`, and `tests/` - every place a
/// `#[path]` attribute plausibly lives in this crate.
fn real_gitsemver_path_inclusion_sites(root: &Path) -> Vec<String> {
    let mut candidates = vec![root.join("build.rs")];
    for top in ["build", "src", "tests"] {
        collect_rs_files(&root.join(top), &mut candidates);
    }
    let mut sites = Vec::new();
    for path in &candidates {
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if rel == "build/gitsemver.rs" {
            continue; // the included file never includes itself
        }
        let includes_gitsemver = content
            .lines()
            .any(|line| is_real_path_attribute_line(line) && line.contains("gitsemver.rs"));
        if includes_gitsemver {
            sites.push(rel);
        }
    }
    sites.sort();
    sites.dedup();
    sites
}

/// The stage1 record's own `reason` prose for both visibility fixes, concatenated - the text
/// that carries the "N `#[path]` inclusion sites" claim this file holds to account. Parsed
/// as `serde_json::Value` deliberately, never a typed struct: this file has no stake in the
/// record's full shape (`tests/compiler_pass_stage1_audit.rs` already owns that), only in
/// this one field's content.
fn stage1_visibility_reason_text(root: &Path) -> String {
    let raw = fs::read_to_string(root.join(RECORD_PATH))
        .unwrap_or_else(|e| panic!("{RECORD_PATH} must exist and be readable: {e}"));
    let record: serde_json::Value = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("{RECORD_PATH} must be valid JSON: {e}"));
    let fixes = record["strict_build"]["visibility_fixes_applied"]
        .as_array()
        .unwrap_or_else(|| {
            panic!("{RECORD_PATH}: strict_build.visibility_fixes_applied must be an array")
        });
    assert!(
        !fixes.is_empty(),
        "{RECORD_PATH}: strict_build.visibility_fixes_applied must not be empty - this unit's \
         own build/gitsemver.rs narrowing is the one real change stage 1 makes"
    );
    fixes
        .iter()
        .map(|fix| fix["reason"].as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn every_real_hash_path_inclusion_site_of_gitsemver_rs_is_named_in_the_stage1_record() {
    let root = repo_root();
    let sites = real_gitsemver_path_inclusion_sites(&root);
    assert!(
        sites.len() >= 2,
        "expected at least build.rs and one other #[path]-inclusion site of build/gitsemver.rs \
         in the tree; the scan found {sites:?} - either the tree genuinely lost a site (a real \
         regression) or this scan itself is broken, both worth stopping for"
    );

    let reason_text = stage1_visibility_reason_text(&root);
    for site in &sites {
        assert!(
            reason_text.contains(site.as_str()),
            "docs/audit/stage1-compiler-pass.json's visibility_fixes_applied reason text does \
             not name {site}, but the tree has a real #[path]-inclusion of build/gitsemver.rs \
             there. The record's own safety narrative is 'nothing outside this item's own \
             defining crate ever reaches it at any of its N #[path] inclusion sites' - that \
             claim only holds if every real site is actually named, so an unnamed site here is \
             a genuine gap in the record (and in the matching prose render_section_4 in \
             tests/simplification_audit.rs adds under section 4.0), not a passing case. Update \
             both to name every site this scan finds: {sites:?}."
        );
    }
}

#[cfg(test)]
mod scan_self_tests {
    use super::*;

    #[test]
    fn a_real_top_level_path_attribute_line_is_recognized() {
        assert!(is_real_path_attribute_line(
            "#[path = \"../build/gitsemver.rs\"]"
        ));
    }

    #[test]
    fn an_indented_path_attribute_line_is_still_recognized() {
        assert!(is_real_path_attribute_line(
            "    #[path = \"../build/gitsemver.rs\"]"
        ));
    }

    #[test]
    fn a_doc_comment_quoting_the_attribute_as_prose_is_not_mistaken_for_a_real_site() {
        // The exact shape tests/code_entity_test_exclusion_periphery.rs's own doc comment
        // writes - the false positive this scan is deliberately built to reject.
        assert!(!is_real_path_attribute_line(
            "/// upward-escaping #[path = \"../build/gitsemver.rs\"]"
        ));
    }

    #[test]
    fn a_line_comment_quoting_the_attribute_as_prose_is_not_mistaken_for_a_real_site() {
        assert!(!is_real_path_attribute_line(
            "// #[path]-included into build.rs and the two test files"
        ));
    }

    #[test]
    fn the_included_file_itself_is_excluded_from_its_own_site_list() {
        let root = repo_root();
        let sites = real_gitsemver_path_inclusion_sites(&root);
        assert!(
            !sites.contains(&"build/gitsemver.rs".to_string()),
            "build/gitsemver.rs must never be counted as including itself: {sites:?}"
        );
    }

    #[test]
    fn the_real_scan_finds_every_known_site_this_units_own_investigation_found() {
        // Pins the scan's own correctness independent of the record cross-check above -
        // these four are confirmed by hand (grep -rn '#\[path.*gitsemver' across the tree,
        // see decision sdet-u87c1-surface-accounting): build.rs (spec 74), src/main.rs
        // (spec 74 c2), tests/gitsemver_derivation.rs (spec 74 c1), and
        // tests/gitsemver_worktree_periphery.rs (spec 74 c1 round 3) - the exact site the
        // record undercounts.
        let root = repo_root();
        let sites = real_gitsemver_path_inclusion_sites(&root);
        for expected in [
            "build.rs",
            "src/main.rs",
            "tests/gitsemver_derivation.rs",
            "tests/gitsemver_worktree_periphery.rs",
        ] {
            assert!(
                sites.iter().any(|s| s == expected),
                "expected {expected} in the real #[path]-inclusion site scan, found: {sites:?}"
            );
        }
    }
}
