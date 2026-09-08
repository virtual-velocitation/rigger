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
//! ROUND 2 (adjudication REJECT on diff `99b73bd..e6faa6f`): the three stale section-6 counts
//! above are now corrected in `render_section_6` (649/60/14). Separately, the adversary found
//! this unit's own FIRST commit had silently hand-patched a citation inside section 5 (owned by
//! criterion 3, not this one) from `dup-0618`/`dup-0625` to `dup-0617`/`dup-0624` - the same
//! population-churn mechanism that caused section 6's own stale counts, this time landing in a
//! different criterion's prose with no disclosing decision and no test guarding it. Disclosed via
//! decision `sdet-u85c4-r2-section5-citations-now-guarded` (supersedes nothing - the original edit
//! was never itself recorded). `section_5_named_dup_id_citations_match_the_committed_catalogs_
//! site_counts` below closes that same gap for section 5's own named `dup-NNNN` citations, so a
//! future population-churn drift there fails a test instead of needing a silent hand-patch again.
//!
//! ROUND 3 (adjudication REJECT on diff `99b73bd..619372d`): two guard-quality defects in this
//! same drift-guard mechanism, both present-tense and both fixed here without touching any
//! numeric citation (the numbers were already correct; only which field each check reads was
//! wrong or missing):
//! 1. `adv-u85c4-r2-section5-2-checks-site-count-for-file-count-citations`: section 5.2's own
//!    loop compared all six of its dup-ids against `catalog_site_counts()`, but the report's own
//!    prose cites `dup-0335`/`dup-0336`/`dup-0361`/`dup-0367` as DISTINCT-FILE counts ("18
//!    files") - only `dup-0366`/`dup-0362` genuinely use site wording ("15-site variant",
//!    "12-site ... companion"). Fixed by splitting the loop: the four file-cited ids now check
//!    `catalog_file_counts()`, the two site-cited ids keep `catalog_site_counts()`. Sites equal
//!    files for all six today, so this changes no pass/fail outcome now - it closes the future
//!    drift blind spot the adversary's failure scenario described (a second site landing inside
//!    an already-listed file would raise the site count while the file count, and the report's
//!    own claim, stayed accurate; the old code would have failed a still-accurate report).
//! 2. `adv-u85c4-r2-tier5-items-14-16-18-citations-unguarded`: section 6 items 14/16/18 each
//!    independently re-type dup-id+count pairs section 5 already states (`dup-0335`/`0336`/
//!    `0361`/`0367`, `dup-0636`, `dup-0583`/`0585`, `dup-0617`/`0624`, `dup-0573`/`0574`,
//!    `dup-0576`), but `section_6_named_dup_id_citations_match_the_committed_catalogs_
//!    site_counts`'s citations array covered only items 3/10-13, contradicting its own doc
//!    comment's claim to check every named citation in section 6. Fixed by extending that same
//!    array (item 14's four file-metric citations, item 18's `dup-0576`) and adding item 16's
//!    "A+B sites" pairs inline (the same shorthand section 5.5 already reads) - closing the
//!    "checks the wrong field" and "claims coverage it does not have" gaps in the guard this
//!    unit's own prior rounds built to catch exactly this class of drift, one tier down.
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

/// Only the fields this file needs per site - deliberately independent of the producer's own
/// `DupSite` shape (see module doc's DELIBERATE INDEPENDENCE note).
#[derive(serde::Deserialize)]
struct MinimalSite {
    file: String,
}

/// Only the fields this file needs to count sites/files per cluster - deliberately independent
/// of the producer's own `DupCluster` shape (see module doc's DELIBERATE INDEPENDENCE note).
#[derive(serde::Deserialize)]
struct MinimalCluster {
    id: String,
    #[serde(default)]
    sites: Vec<MinimalSite>,
}

fn load_clusters() -> Vec<MinimalCluster> {
    let raw = fs::read_to_string(repo_root().join(CATALOG_PATH))
        .unwrap_or_else(|e| panic!("{CATALOG_PATH} is missing or unreadable ({e})"));
    serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("{CATALOG_PATH} does not deserialize as a cluster list: {e}"))
}

/// Per-cluster site count (every catalogued occurrence, files or not).
fn catalog_site_counts() -> HashMap<String, usize> {
    load_clusters()
        .into_iter()
        .map(|c| (c.id, c.sites.len()))
        .collect()
}

/// Per-cluster DISTINCT-file count - smaller than the site count whenever one file holds more
/// than one site (e.g. a cluster with two sites in the same file counts as 1 file, 2 sites).
/// Section 5's own prose cites this metric for several clusters (e.g. "5 files") where the
/// site count itself differs (that cluster's sites span fewer files than sites).
fn catalog_file_counts() -> HashMap<String, usize> {
    load_clusters()
        .into_iter()
        .map(|c| {
            let files: std::collections::HashSet<String> =
                c.sites.into_iter().map(|s| s.file).collect();
            (c.id, files.len())
        })
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

/// Isolates one heading's own span (from the first line starting with `heading` up to, but not
/// including, the next line starting with any of `stop_prefixes`) so citation anchors never need
/// to be unique across the WHOLE report - only within their own span, mirroring the "one owner
/// per span" contract `replace_section_*` itself relies on. The one shared building block behind
/// both `item_block` (section 6's numbered items) and `sub_section_block` (section 5's `### 5.N`
/// subsections) - one boundary-scoping algorithm, not two parallel copies.
fn heading_block<'a>(
    report: &'a str,
    lines: &[(usize, &'a str)],
    heading: &str,
    stop_prefixes: &[&str],
) -> &'a str {
    let idx = lines
        .iter()
        .position(|(_, l)| l.starts_with(heading))
        .unwrap_or_else(|| panic!("no line starts with {heading:?} in {REPORT_PATH}"));
    let start = lines[idx].0;
    let end = lines[idx + 1..]
        .iter()
        .find(|(_, l)| stop_prefixes.iter().any(|p| l.starts_with(p)))
        .map(|(pos, _)| *pos)
        .unwrap_or(report.len());
    &report[start..end]
}

/// Isolates one numbered plan item's own paragraph (from its `#### N. ` heading up to, but not
/// including, the next `#### `/`### `/`## ` line).
fn item_block<'a>(report: &'a str, lines: &[(usize, &'a str)], n: u32) -> &'a str {
    let marker = format!("#### {n}. ");
    heading_block(report, lines, &marker, &["#### ", "### ", "## "])
}

/// Isolates one `### 5.N ...` subsection of section 5 (up to the next `### ` or `## ` line).
fn sub_section_block<'a>(report: &'a str, lines: &[(usize, &'a str)], heading: &str) -> &'a str {
    heading_block(report, lines, heading, &["### ", "## "])
}

/// Extracts the raw text sitting between `before`'s first occurrence in `block` and the first
/// occurrence of `after` following it - both anchors are copied verbatim from the report's own
/// current prose around a citation, so this only ever fails when the wording around the
/// citation itself changed, never silently.
fn substring_between<'a>(block: &'a str, before: &str, after: &str) -> &'a str {
    let start = block
        .find(before)
        .unwrap_or_else(|| panic!("anchor {before:?} not found in item block {block:?}"));
    let tail = &block[start + before.len()..];
    let end = tail
        .find(after)
        .unwrap_or_else(|| panic!("anchor {after:?} not found after {before:?} in {block:?}"));
    &tail[..end]
}

/// Extracts the plain integer sitting between `before` and `after` inside `block` - this only
/// ever fails when the digits between them stop being a bare number (the wording around the
/// citation changed) rather than when only the cited number itself is wrong.
fn number_between(block: &str, before: &str, after: &str) -> u32 {
    let digits = substring_between(block, before, after);
    digits.trim().parse::<u32>().unwrap_or_else(|e| {
        panic!("expected a bare number between {before:?} and {after:?}, found {digits:?}: {e}")
    })
}

/// Extracts an `"A<sep>B"`-style pair (the report's own shorthand for "one cluster of A units,
/// its companion cluster of B units") sitting between `before` and `after` inside `block`, split
/// on the literal `sep` that actually separates the two numbers in THIS citation's own prose -
/// `"+"` for an "A+B sites" pair, `" files, "` for an "A files, B sites)" pair, `"-file/"` for an
/// "A-file/B-site" pair, and so on. Parameterizing the separator (rather than hardcoding one) is
/// what lets `before`/`after` stay pure prose anchors that never embed either number: a citation
/// with two counts is extracted as ONE span and split, instead of two separate `number_between`
/// calls whose anchors would otherwise have to embed the other count's current digit to stay
/// unique - which breaks `substring_between`'s own contract (only wording changes should move an
/// anchor) and makes a single-axis drift on one count panic on the OTHER count's anchor instead
/// of reporting a clean mismatch.
fn number_pair_between(block: &str, before: &str, after: &str, sep: &str) -> (u32, u32) {
    let raw = substring_between(block, before, after);
    let (a, b) = raw.split_once(sep).unwrap_or_else(|| {
        panic!("expected an \"A{sep}B\" pair between {before:?} and {after:?}, found {raw:?}")
    });
    let parse = |s: &str| {
        s.trim().parse::<u32>().unwrap_or_else(|e| {
            panic!("expected a bare number in pair {raw:?} between {before:?} and {after:?}: {e}")
        })
    };
    (parse(a), parse(b))
}

/// Finds the bare integer nearest to, and immediately preceding (skipping only non-digit filler
/// text such as "-site" or " files"), `marker`'s first occurrence in `block` - the mirror image
/// of `number_between`, for the report's own "N files (`dup-NNNN`" phrasing where the count
/// comes BEFORE the cluster id's own citation rather than after it.
fn number_immediately_before(block: &str, marker: &str) -> u32 {
    let idx = block
        .find(marker)
        .unwrap_or_else(|| panic!("anchor {marker:?} not found in block {block:?}"));
    let head = &block[..idx];
    let bytes = head.as_bytes();
    let mut end = bytes.len();
    while end > 0 && !bytes[end - 1].is_ascii_digit() {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && bytes[start - 1].is_ascii_digit() {
        start -= 1;
    }
    assert!(start < end, "no number found before {marker:?} in {head:?}");
    head[start..end].parse::<u32>().unwrap_or_else(|e| {
        panic!(
            "expected a bare number before {marker:?}, found {:?}: {e}",
            &head[start..end]
        )
    })
}

/// One citation of a named `dup-NNNN` cluster's count inside one numbered plan item. `metric`
/// is `"site"` or `"file"`, matching whichever metric the report's own prose actually names at
/// that citation (see `catalog_site_counts`/`catalog_file_counts`).
struct Citation {
    item: u32,
    dup_id: &'static str,
    metric: &'static str,
    before: &'static str,
    after: &'static str,
}

/// THE CROSS-ARTIFACT CONTRACT: section 6's own Done-when text is "cites sections 1-5 and adds
/// no new findings" - every named `dup-NNNN` citation in section 6 that carries an explicit
/// site or file count is checked here against the count the SAME id carries in the committed
/// `docs/audit/duplication-catalog.json`, independent of however section 6's own prose was
/// authored. This covers tiers 1-5 (items 3, 10-14, 16, 18); items 15/17/19 name clusters only
/// by bare count ("27,074 lines", "177 clusters", "327 clusters") with no `dup-NNNN` id
/// attached to a specific number, so there is nothing for this guard to cross-check there.
#[test]
fn section_6_named_dup_id_citations_match_the_committed_catalogs_site_counts() {
    let report = read_report();
    let lines = lines_with_offsets(&report);
    let sites = catalog_site_counts();
    let files = catalog_file_counts();

    let citations = [
        Citation {
            item: 3,
            dup_id: "dup-0125",
            metric: "site",
            before: "capstone previously caught (`dup-0125`, ",
            after: " sites: `src/dash.rs`",
        },
        Citation {
            item: 3,
            dup_id: "dup-0124",
            metric: "site",
            before: ", plus ",
            after: " raw `/proc`-path string literals scattered across `src/dash.rs`, \
                    `src/main.rs`, `src/reap.rs` and three test files with no shared composer \
                    (`dup-0124`)",
        },
        Citation {
            item: 10,
            dup_id: "dup-0051",
            metric: "site",
            before: "#### 10. Consolidate the ",
            after: " `.rigger`-path string-literal sites (`dup-0051`)",
        },
        Citation {
            item: 10,
            dup_id: "dup-0051",
            metric: "site",
            before: "`proposed_home`) every one of the ",
            after: " sites routes through instead of building its own literal.",
        },
        Citation {
            item: 10,
            dup_id: "dup-0051",
            metric: "site",
            before: "Expected line delta: negative - ",
            after: " literal compositions collapse toward one helper's call sites; the helper \
                    itself is small.",
        },
        Citation {
            item: 10,
            dup_id: "dup-0051",
            metric: "site",
            before: "full-suite green run, not hand-editing ",
            after: " sites.",
        },
        Citation {
            item: 11,
            dup_id: "dup-0006",
            metric: "site",
            before: "#### 11. Consolidate the ",
            after: " `Command::new` call sites (`dup-0006`)",
        },
        Citation {
            item: 11,
            dup_id: "dup-0006",
            metric: "site",
            before: "Risk: medium-high - several of these ",
            after: " sites sit inside `src/budget.rs`'s",
        },
        Citation {
            item: 12,
            dup_id: "dup-0105",
            metric: "site",
            before: "#### 12. Consolidate the ",
            after: " sqlite `Connection::open` call sites (`dup-0105`)",
        },
        Citation {
            item: 12,
            dup_id: "dup-0105",
            metric: "site",
            before: "Expected line delta: negative - ",
            after: " open calls collapse toward one function.",
        },
        Citation {
            item: 13,
            dup_id: "dup-0205",
            metric: "site",
            before: "#### 13. Consolidate the ",
            after: " error-shaping helper sites (`dup-0205`)",
        },
        // Item 14 (tier 5, section 5.2's four headline fixtures): the report cites these as
        // DISTINCT-FILE counts ("18 files"), matching section 5.2's own wording, so these use
        // the `file` metric - not `site` - exactly mirroring the section-5.2 fix above.
        Citation {
            item: 14,
            dup_id: "dup-0335",
            metric: "file",
            before: "`page_script` (`dup-0335`, ",
            after: " files)",
        },
        Citation {
            item: 14,
            dup_id: "dup-0336",
            metric: "file",
            before: "`node_available` (`dup-0336`, ",
            after: " files)",
        },
        Citation {
            item: 14,
            dup_id: "dup-0361",
            metric: "file",
            before: "`temp_project` (`dup-0361`, ",
            after: " files)",
        },
        Citation {
            item: 14,
            dup_id: "dup-0367",
            metric: "file",
            before: "`run_stream_identity` (`dup-0367`, ",
            after: " files)",
        },
        // Item 18's one single-value citation (its other named id, `dup-0576`, is the only
        // one item 18 cites with an explicit count - see the item-16 pairs handled below).
        Citation {
            item: 18,
            dup_id: "dup-0576",
            metric: "site",
            before: "`dup-0576` (",
            after: " sites, `tests/no_os_kill_test_helper_periphery.rs`",
        },
    ];

    let mut mismatches = Vec::new();
    for c in &citations {
        let block = item_block(&report, &lines, c.item);
        let cited = number_between(block, c.before, c.after);
        let counts = if c.metric == "file" { &files } else { &sites };
        record_mismatch(
            &mut mismatches,
            &format!("item {}", c.item),
            c.dup_id,
            c.metric,
            cited,
            counts,
        );
    }

    // Item 16's remaining citations use the report's own "A+B sites" shorthand for a cluster's
    // companion pair - the same shape `section_5_named_dup_id_citations_match_the_committed_
    // catalogs_site_counts` already checks in section 5.5, reused here for section 6's own
    // independently hand-typed copies of the identical dup-ids+counts (see
    // `adv-u85c4-r2-tier5-items-14-16-18-citations-unguarded`: these were entirely unchecked
    // before this fix).
    let item_16 = item_block(&report, &lines, 16);
    let cited = number_between(item_16, "`dup-0636` (", " sites, `tests/spec_lint.rs`");
    record_mismatch(
        &mut mismatches,
        "item 16",
        "dup-0636",
        "site",
        cited,
        &sites,
    );
    let (a, b) = number_pair_between(
        item_16,
        "`dup-0583`/`dup-0585` (",
        " sites, `tests/reap_before_removal_audit.rs`",
        "+",
    );
    record_mismatch(&mut mismatches, "item 16", "dup-0583", "site", a, &sites);
    record_mismatch(&mut mismatches, "item 16", "dup-0585", "site", b, &sites);
    let (a, b) = number_pair_between(
        item_16,
        "`dup-0617`/`dup-0624` (",
        " sites, `tests/simplification_audit.rs`",
        "+",
    );
    record_mismatch(&mut mismatches, "item 16", "dup-0617", "site", a, &sites);
    record_mismatch(&mut mismatches, "item 16", "dup-0624", "site", b, &sites);
    let (a, b) = number_pair_between(
        item_16,
        "`dup-0573`/`dup-0574` (",
        " sites, `tests/no_os_kill_audit.rs`",
        "+",
    );
    record_mismatch(&mut mismatches, "item 16", "dup-0573", "site", a, &sites);
    record_mismatch(&mut mismatches, "item 16", "dup-0574", "site", b, &sites);

    assert!(
        mismatches.is_empty(),
        "section 6 cites stale site/file counts that no longer match the committed duplication \
         catalog (section 6's own Done-when text requires accurately citing sections 1-5):\n{}",
        mismatches.join("\n")
    );
}

/// Appends a mismatch line to `mismatches` when `cited` disagrees with `counts[dup_id]` -
/// shared by every section-5 citation check below regardless of how the number was located
/// (immediately-before, between two anchors, or half of an "A+B" pair).
fn record_mismatch(
    mismatches: &mut Vec<String>,
    location: &str,
    dup_id: &str,
    metric_name: &str,
    cited: u32,
    counts: &HashMap<String, usize>,
) {
    let actual = *counts.get(dup_id).unwrap_or_else(|| {
        panic!("{dup_id} is cited in {location} but has no cluster in {CATALOG_PATH}")
    }) as u32;
    if cited != actual {
        mismatches.push(format!(
            "{location}: {dup_id} cited as {cited} {metric_name}(s) in {REPORT_PATH}, but \
             {CATALOG_PATH} carries {actual} {metric_name}(s)"
        ));
    }
}

/// THE CROSS-ARTIFACT CONTRACT, widened to section 5 (closing the same blast-radius gap for the
/// criterion that actually caused the drift `section_6_...` above catches only for section 6):
/// every named `dup-NNNN` citation in section 5 that carries an explicit site or file count is
/// checked here against the committed `docs/audit/duplication-catalog.json`, independent of
/// however section 5's own prose was authored. See decision
/// `sdet-u85c4-r2-section5-citations-now-guarded` for why this exists: this unit's own first
/// commit silently hand-patched one of these citations (`dup-0618`/`dup-0625` ->
/// `dup-0617`/`dup-0624`) with no test guarding section 5's citations against the same
/// population-churn drift section 6's own test already caught for itself.
#[test]
fn section_5_named_dup_id_citations_match_the_committed_catalogs_site_counts() {
    let report = read_report();
    let lines = lines_with_offsets(&report);
    let sites = catalog_site_counts();
    let files = catalog_file_counts();
    let mut mismatches = Vec::new();

    // 5.2: shared-fixture citations - the report cites the count BEFORE naming the cluster id
    // ("independently redefined in 18 files (`dup-0335`...)"), so these use the backward scan.
    // The report's own wording splits by metric here: `dup-0335`/`dup-0336`/`dup-0361`/
    // `dup-0367` are cited as DISTINCT-FILE counts ("18 different files" / "the same 18
    // files"), while `dup-0366`/`dup-0362` are genuinely cited as SITE counts ("15-site
    // variant", "12-site ... companion helper") - each group is checked against the metric
    // its own prose actually names, not uniformly against site counts (see
    // `adv-u85c4-r2-section5-2-checks-site-count-for-file-count-citations`: checking the wrong
    // field passes today only because sites == files for these clusters by coincidence).
    let sec_5_2 = sub_section_block(&report, &lines, "### 5.2 ");
    for dup_id in ["dup-0335", "dup-0336", "dup-0361", "dup-0367"] {
        let marker = format!("(`{dup_id}`");
        let cited = number_immediately_before(sec_5_2, &marker);
        record_mismatch(
            &mut mismatches,
            "section 5.2",
            dup_id,
            "file",
            cited,
            &files,
        );
    }
    for dup_id in ["dup-0366", "dup-0362"] {
        let marker = format!("(`{dup_id}`");
        let cited = number_immediately_before(sec_5_2, &marker);
        record_mismatch(
            &mut mismatches,
            "section 5.2",
            dup_id,
            "site",
            cited,
            &sites,
        );
    }

    // 5.4: duplicated-helper citations - the cluster id comes first, then its file/site counts.
    // dup-0339 and dup-0395 each cite an "N files, M sites" pair in one span: extracted as ONE
    // number_pair_between call (sep " files, ") rather than two separate number_between calls,
    // so neither anchor ever embeds the other count's digit - see number_pair_between's doc
    // comment for why a digit-embedded anchor is unsound as a drift guard.
    let sec_5_4 = sub_section_block(&report, &lines, "### 5.4 ");
    let (file_cited, site_cited) = number_pair_between(
        sec_5_4,
        "architecture-integrity checks, ",
        " sites);",
        " files, ",
    );
    record_mismatch(
        &mut mismatches,
        "section 5.4",
        "dup-0339",
        "file",
        file_cited,
        &files,
    );
    record_mismatch(
        &mut mismatches,
        "section 5.4",
        "dup-0339",
        "site",
        site_cited,
        &sites,
    );
    let (file_cited, site_cited) = number_pair_between(
        sec_5_4,
        "`tests/step_attention_periphery.rs`, ",
        " sites); `dup-0369`",
        " files, ",
    );
    record_mismatch(
        &mut mismatches,
        "section 5.4",
        "dup-0395",
        "file",
        file_cited,
        &files,
    );
    record_mismatch(
        &mut mismatches,
        "section 5.4",
        "dup-0395",
        "site",
        site_cited,
        &sites,
    );
    let cited = number_between(sec_5_4, "fold-application helpers, ", " files); `dup-0461`");
    record_mismatch(
        &mut mismatches,
        "section 5.4",
        "dup-0454",
        "file",
        cited,
        &files,
    );
    let cited = number_between(
        sec_5_4,
        "single-field constructor helpers, ",
        " files); `dup-0657`",
    );
    record_mismatch(
        &mut mismatches,
        "section 5.4",
        "dup-0461",
        "file",
        cited,
        &files,
    );
    let cited = number_between(sec_5_4, "two-line accessor helpers, ", " files).");
    record_mismatch(
        &mut mismatches,
        "section 5.4",
        "dup-0657",
        "file",
        cited,
        &files,
    );

    // 5.4 (second citations): `dup-0366` and `dup-0367` are each named a SECOND,
    // textually-independent time later in this same subsection's `dup-0339` paragraph ("a
    // companion, 15-file/15-site variant of 5.2's `run_stream_identity` fixture, alongside
    // `dup-0367`'s 18-file version)") - a distinct citation location from section 5.2's
    // site-only check above (scoped to `### 5.2`'s own span) and from section 6 item 14's
    // file-only check (a different citation site entirely), so neither one guards these. See
    // decision `sdet-u85c4-r4-section5-4-second-citations-guarded`. The "N-file/M-site" pair is
    // extracted as ONE number_pair_between call (sep "-file/") - not two number_between calls -
    // so neither anchor embeds the other count's digit; see number_pair_between's doc comment.
    let (file_cited, site_cited) =
        number_pair_between(sec_5_4, "a companion, ", "-site variant", "-file/");
    record_mismatch(
        &mut mismatches,
        "section 5.4 (second citation)",
        "dup-0366",
        "file",
        file_cited,
        &files,
    );
    record_mismatch(
        &mut mismatches,
        "section 5.4 (second citation)",
        "dup-0366",
        "site",
        site_cited,
        &sites,
    );
    let cited = number_between(sec_5_4, "alongside `dup-0367`'s ", "-file version)");
    record_mismatch(
        &mut mismatches,
        "section 5.4 (second citation)",
        "dup-0367",
        "file",
        cited,
        &files,
    );

    // 5.5: table-driven-family citations - single "N sites" citations and "A+B sites" pairs.
    let sec_5_5 = sub_section_block(&report, &lines, "### 5.5 ");
    let cited = number_between(
        sec_5_5,
        "single largest anywhere in the suite: `dup-0636` (near, ",
        " sites, all in `tests/spec_lint.rs`",
    );
    record_mismatch(
        &mut mismatches,
        "section 5.5",
        "dup-0636",
        "site",
        cited,
        &sites,
    );
    let (a, b) = number_pair_between(
        sec_5_5,
        "Other large families: `dup-0583`/`dup-0585` (",
        " sites, `tests/reap_before_removal_audit.rs`",
        "+",
    );
    record_mismatch(
        &mut mismatches,
        "section 5.5",
        "dup-0583",
        "site",
        a,
        &sites,
    );
    record_mismatch(
        &mut mismatches,
        "section 5.5",
        "dup-0585",
        "site",
        b,
        &sites,
    );
    let (a, b) = number_pair_between(
        sec_5_5,
        "`dup-0617`/`dup-0624` (",
        " sites, `tests/simplification_audit.rs`",
        "+",
    );
    record_mismatch(
        &mut mismatches,
        "section 5.5",
        "dup-0617",
        "site",
        a,
        &sites,
    );
    record_mismatch(
        &mut mismatches,
        "section 5.5",
        "dup-0624",
        "site",
        b,
        &sites,
    );
    let (a, b) = number_pair_between(
        sec_5_5,
        "`dup-0573`/`dup-0574` (",
        " sites, `tests/no_os_kill_audit.rs`",
        "+",
    );
    record_mismatch(
        &mut mismatches,
        "section 5.5",
        "dup-0573",
        "site",
        a,
        &sites,
    );
    record_mismatch(
        &mut mismatches,
        "section 5.5",
        "dup-0574",
        "site",
        b,
        &sites,
    );
    let cited = number_between(
        sec_5_5,
        "`dup-0576` (",
        " sites, `tests/no_os_kill_test_helper_periphery.rs`",
    );
    record_mismatch(
        &mut mismatches,
        "section 5.5",
        "dup-0576",
        "site",
        cited,
        &sites,
    );

    assert!(
        mismatches.is_empty(),
        "section 5 cites stale site/file counts that no longer match the committed duplication \
         catalog:\n{}",
        mismatches.join("\n")
    );
}
