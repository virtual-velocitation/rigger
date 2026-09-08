//! Spec 85 criterion 4 (`u85c4`), SDET periphery layer: a cross-artifact contract test for
//! `docs/audit/2026-09-simplification-audit.md`'s section 6 (the prioritized plan) and for the
//! report's own top-level heading-boundary structure.
//!
//! Boundary-surface accounting (mechanical probes against base
//! `99b73bdb44a1e0488a6b9b18b35a693b619b2e1c`, see decision `sdet-u85c4-surface-accounting`):
//! the pub-API, trait-impl, CLI, and event/serialized-form probes all came back empty - this
//! unit changes only `tests/simplification_audit.rs`, adds no `pub` item, no trait impl, no CLI
//! surface, and no new `derive(Serialize/Deserialize)` type. The cross-module-seam/fold-arm
//! probe is NOT empty, on a plain read of the diff rather than a grep:
//!
//! 1. A new private helper, `find_heading`, replaces every `replace_section_*` function's own
//!    unanchored `str::find("## N. ")` call with a line-start-anchored search - a shared
//!    boundary-detection algorithm all four report-owning criteria's generators (sections 1, 2,
//!    3-5 and 6) now route through. Its own doc comment describes a real corruption class: a
//!    `#### N. ` sub-heading's tail reads as `## N. ` from its third character on, so an
//!    unanchored search can land on that false, embedded position instead of the real heading
//!    and silently truncate or overwrite the report. None of the unit's own new inline tests
//!    exercise this exact false-match shape directly (they cover `replace_section_6`'s ordinary
//!    span-removal contract, not `find_heading`'s own anchoring guarantee) - TESTED here by an
//!    independent, non-reused heading-offset scan of the REAL committed report (never calling
//!    `find_heading` or any `replace_section_*`), proving the six top-level headings land in
//!    the correct order on the actual artifact, not merely that the generator agrees with
//!    itself.
//! 2. Section 6 is a new CONSUMER of sections 1-2's data: its own text states "every citation
//!    below points at a claim already recorded in section 1 ... section 2 ... or sections 3-5's
//!    own prose" - a real integration seam between the hand-authored `.md` prose and the
//!    machine-generated `docs/audit/duplication-catalog.json`. Cross-checking that claim
//!    mechanically (rather than trusting the prose, or decision `u85c4-section6-plan-structure`'s
//!    own "all figures pulled directly from the committed ... duplication-catalog.json" claim)
//!    found THREE citations that do not match the committed catalog: `dup-0051` cited as 645
//!    sites (catalog: 648), `dup-0124` cited as 56 sites (catalog: 59), `dup-0125` cited as 13
//!    sites (catalog: 14) - all three read as STALE counts that predate this unit's own final
//!    regeneration pass (section 2 of the SAME report already states the correct 648/59/14 for
//!    these same three clusters). TESTED here: every numbered-tier citation of a named
//!    `dup-NNNN` id in section 6 that carries an explicit site count is cross-checked against
//!    the actual site count in the committed catalog; the three mismatches above make this test
//!    fail today - a genuine boundary bug for the implementer to fix, never a reason to weaken
//!    the check. `dup-0198` is excluded: section 6 names it without citing a bare site count.
//!
//! DELIBERATE INDEPENDENCE: this file never calls `tests/simplification_audit.rs`'s private
//! `find_heading` / `replace_section_*` / `render_section_6` (integration test binaries cannot
//! see another file's private items anyway) and declares its own minimal cluster shape rather
//! than importing `DupCluster` - the same position a real downstream reader of both committed
//! artifacts is in.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

const REPORT_PATH: &str = "docs/audit/2026-09-simplification-audit.md";
const CATALOG_PATH: &str = "docs/audit/duplication-catalog.json";

/// The repo root this test binary was compiled from - never the process CWD (same convention
/// as `tests/simplification_audit.rs::repo_root` and its sibling periphery files).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_report() -> String {
    fs::read_to_string(repo_root().join(REPORT_PATH))
        .unwrap_or_else(|e| panic!("{REPORT_PATH} is missing or unreadable ({e})"))
}

/// Only the fields this file needs to count sites per cluster - deliberately independent of
/// the producer's own `DupCluster` shape (see module doc's DELIBERATE INDEPENDENCE note).
#[derive(serde::Deserialize)]
struct MinimalCluster {
    id: String,
    #[serde(default)]
    sites: Vec<serde_json::Value>,
}

fn catalog_site_counts() -> HashMap<String, usize> {
    let raw = fs::read_to_string(repo_root().join(CATALOG_PATH))
        .unwrap_or_else(|e| panic!("{CATALOG_PATH} is missing or unreadable ({e})"));
    let clusters: Vec<MinimalCluster> = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("{CATALOG_PATH} does not deserialize as a cluster list: {e}"));
    clusters
        .into_iter()
        .map(|c| (c.id, c.sites.len()))
        .collect()
}

/// `text` split into `(byte_offset, line_including_its_newline)` pairs - the one independent
/// building block both tests below use to reason about line-start boundaries without ever
/// calling the producer's own `find_heading`.
fn lines_with_offsets(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    for line in text.split_inclusive('\n') {
        out.push((pos, line));
        pos += line.len();
    }
    out
}

/// THE ANCHORING PROOF: the six top-level `## N. ` headings, found by direct line-start
/// inspection (never `find_heading`), appear exactly once each and in ascending byte-offset
/// order in the REAL committed report - the real-world guarantee `find_heading`'s anchoring fix
/// exists to provide. A regression that reintroduced an unanchored heading search and corrupted
/// the committed file (duplicated, reordered, or dropped a top-level section) would fail this
/// directly, independent of whatever internal function produced the file.
#[test]
fn the_six_top_level_sections_appear_exactly_once_each_in_ascending_order() {
    let report = read_report();
    let lines = lines_with_offsets(&report);

    let mut offsets = Vec::with_capacity(6);
    for n in 1..=6u32 {
        let marker = format!("## {n}. ");
        let hits: Vec<usize> = lines
            .iter()
            .filter(|(_, l)| l.starts_with(marker.as_str()))
            .map(|(pos, _)| *pos)
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "{REPORT_PATH} should have exactly one line starting with {marker:?}, found {}",
            hits.len()
        );
        offsets.push(hits[0]);
    }

    let mut sorted = offsets.clone();
    sorted.sort_unstable();
    assert_eq!(
        offsets, sorted,
        "{REPORT_PATH}'s six top-level sections are not in ascending byte-offset order: {offsets:?} \
         - a heading search that is not anchored to a genuine line start could land on a \
         heading-shaped substring embedded earlier in the file instead of the real heading"
    );
}

/// Isolates one numbered plan item's own paragraph (from its `#### N. ` heading up to, but not
/// including, the next `#### `/`### `/`## ` line) so the citation anchors below never need to
/// be unique across the WHOLE report - only within their own item, mirroring the "one owner per
/// span" contract `replace_section_*` itself relies on.
fn item_block<'a>(report: &'a str, lines: &[(usize, &'a str)], n: u32) -> &'a str {
    let marker = format!("#### {n}. ");
    let idx = lines
        .iter()
        .position(|(_, l)| l.starts_with(marker.as_str()))
        .unwrap_or_else(|| panic!("no line starts with {marker:?} in {REPORT_PATH}"));
    let start = lines[idx].0;
    let end = lines[idx + 1..]
        .iter()
        .find(|(_, l)| l.starts_with("#### ") || l.starts_with("### ") || l.starts_with("## "))
        .map(|(pos, _)| *pos)
        .unwrap_or(report.len());
    &report[start..end]
}

/// Extracts the plain integer sitting between `before` and `after` inside `block` - both
/// anchors are copied verbatim from the report's own current prose around a citation, so this
/// only ever fails when the digits between them stop being a bare number (the wording around
/// the citation changed) rather than when only the cited number itself is wrong.
fn number_between(block: &str, before: &str, after: &str) -> u32 {
    let start = block
        .find(before)
        .unwrap_or_else(|| panic!("anchor {before:?} not found in item block {block:?}"));
    let tail = &block[start + before.len()..];
    let end = tail
        .find(after)
        .unwrap_or_else(|| panic!("anchor {after:?} not found after {before:?} in {block:?}"));
    let digits = &tail[..end];
    digits.trim().parse::<u32>().unwrap_or_else(|e| {
        panic!("expected a bare number between {before:?} and {after:?}, found {digits:?}: {e}")
    })
}

/// One citation of a named `dup-NNNN` cluster's site count inside one numbered plan item.
struct Citation {
    item: u32,
    dup_id: &'static str,
    before: &'static str,
    after: &'static str,
}

/// THE CROSS-ARTIFACT CONTRACT: section 6's own Done-when text is "cites sections 1-5 and adds
/// no new findings" - every named `dup-NNNN` citation that carries an explicit site count is
/// checked here against the count the SAME id carries in the committed
/// `docs/audit/duplication-catalog.json`, independent of however section 6's own prose was
/// authored.
#[test]
fn section_6_named_dup_id_citations_match_the_committed_catalogs_site_counts() {
    let report = read_report();
    let lines = lines_with_offsets(&report);
    let counts = catalog_site_counts();

    let citations = [
        Citation {
            item: 3,
            dup_id: "dup-0125",
            before: "capstone previously caught (`dup-0125`, ",
            after: " sites: `src/dash.rs`",
        },
        Citation {
            item: 3,
            dup_id: "dup-0124",
            before: ", plus ",
            after: " raw `/proc`-path string literals scattered across `src/dash.rs`, \
                    `src/main.rs`, `src/reap.rs` and three test files with no shared composer \
                    (`dup-0124`)",
        },
        Citation {
            item: 10,
            dup_id: "dup-0051",
            before: "#### 10. Consolidate the ",
            after: " `.rigger`-path string-literal sites (`dup-0051`)",
        },
        Citation {
            item: 10,
            dup_id: "dup-0051",
            before: "`proposed_home`) every one of the ",
            after: " sites routes through instead of building its own literal.",
        },
        Citation {
            item: 10,
            dup_id: "dup-0051",
            before: "Expected line delta: negative - ",
            after: " literal compositions collapse toward one helper's call sites; the helper \
                    itself is small.",
        },
        Citation {
            item: 10,
            dup_id: "dup-0051",
            before: "full-suite green run, not hand-editing ",
            after: " sites.",
        },
        Citation {
            item: 11,
            dup_id: "dup-0006",
            before: "#### 11. Consolidate the ",
            after: " `Command::new` call sites (`dup-0006`)",
        },
        Citation {
            item: 11,
            dup_id: "dup-0006",
            before: "Risk: medium-high - several of these ",
            after: " sites sit inside `src/budget.rs`'s",
        },
        Citation {
            item: 12,
            dup_id: "dup-0105",
            before: "#### 12. Consolidate the ",
            after: " sqlite `Connection::open` call sites (`dup-0105`)",
        },
        Citation {
            item: 12,
            dup_id: "dup-0105",
            before: "Expected line delta: negative - ",
            after: " open calls collapse toward one function.",
        },
        Citation {
            item: 13,
            dup_id: "dup-0205",
            before: "#### 13. Consolidate the ",
            after: " error-shaping helper sites (`dup-0205`)",
        },
    ];

    let mut mismatches = Vec::new();
    for c in &citations {
        let block = item_block(&report, &lines, c.item);
        let cited = number_between(block, c.before, c.after);
        let actual = *counts.get(c.dup_id).unwrap_or_else(|| {
            panic!(
                "{} is cited in section 6 item {} but has no cluster in {CATALOG_PATH}",
                c.dup_id, c.item
            )
        }) as u32;
        if cited != actual {
            mismatches.push(format!(
                "item {}: {} cited as {} site(s) in {REPORT_PATH}, but {CATALOG_PATH} carries \
                 {} site(s)",
                c.item, c.dup_id, cited, actual
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "section 6 cites stale site counts that no longer match the committed duplication \
         catalog (section 6's own Done-when text requires accurately citing sections 1-5):\n{}",
        mismatches.join("\n")
    );
}
