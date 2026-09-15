//! Spec 85 criterion 4 (`u85c4`), SDET periphery layer: a cross-artifact contract test for
//! `docs/audit/2026-09-simplification-audit.md` - the report's own top-level heading-boundary
//! structure, and (ROUND 6, see below) every `dup-NNNN` cluster citation anywhere in the report.
//!
//! Boundary-surface accounting (mechanical probes against base
//! `99b73bdb44a1e0488a6b9b18b35a693b619b2e1c`, see decision `sdet-u85c4-surface-accounting`):
//! the pub-API, trait-impl, CLI, and event/serialized-form probes all came back empty - this
//! unit changes only `tests/simplification_audit.rs`, adds no `pub` item, no trait impl, no CLI
//! surface, and no new `derive(Serialize/Deserialize)` type. The cross-module-seam/fold-arm
//! probe is NOT empty: a new private helper, `find_heading`, replaces every `replace_section_*`
//! function's own unanchored `str::find("## N. ")` call with a line-start-anchored search - a
//! shared boundary-detection algorithm all four report-owning criteria's generators now route
//! through. TESTED here by `the_six_top_level_sections_appear_exactly_once_each_in_ascending_
//! order`, an independent, non-reused heading-offset scan of the REAL committed report (never
//! calling `find_heading` or any `replace_section_*`), proving the six top-level headings land
//! in the correct order on the actual artifact, not merely that the generator agrees with
//! itself.
//!
//! ROUNDS 1-5 (adjudication REJECT on diffs through `619372d..099bb84..c2fbc07`): built and then
//! repeatedly hardened a citation drift-guard, one hand-anchored check per named `dup-NNNN`
//! citation in sections 5 and 6 - each round closed a real gap (stale counts, the wrong metric
//! checked, a citation nobody guarded, a `report:regeneration disclosed`) but round 5's own
//! adjudication (`adj-u85c4-r5-verdict-reject`) found the anchor-per-citation MECHANISM itself
//! was the recurring defect source: two checks whose anchors embedded each other's digit
//! (`periphery.rs:678,687` pre-fix) coupled a single-axis drift on one metric into an
//! uninformative panic that swallowed the other metric's own correctly-computed mismatch - and
//! the identical coupling, unnoticed, already lived in a round-2-approved check for a different
//! `dup-id`. Operator decision `d-u85c4-round6-remedy-is-the-generic-guard` (round 6, the final
//! attempt) named the mechanism itself as the defect and mandated its replacement: delete every
//! section-scoped, hand-anchored check and replace them with ONE pass that mechanically finds
//! every `dup-NNNN` citation anywhere in the whole report and checks it against the committed
//! catalog - "by construction" ruling out anchor coupling, embedded digits, and an unguarded
//! citation location as findings against a round that lands it.
//!
//! ROUND 6 (this round): `every_dup_id_citation_anywhere_in_the_report_matches_the_committed_
//! catalog` below is that one pass. It never types a citation's own surrounding prose as an
//! anchor (the round 1-5 mechanism this replaces); it mechanically walks the WHOLE report text
//! (`scan_citations`), so it has no "which section did I remember to cover" gap by construction,
//! and it reuses `number_pair_between` (already generalized in round 5) as its sole two-number
//! extractor rather than inventing a second one. See `scan_citations`'s own doc comment for the
//! extraction algorithm and the concrete false-attribution bugs its two safety properties (a
//! bounded number-to-keyword gap, and a guard against a hyphenated "A-B" range read as a
//! false per-id count) were built to close - each verified against the real committed report
//! before landing (a corrupted single digit is caught; the correct report yields zero
//! mismatches; a text like "6-8 files" attached to two different cluster ids, or "2-5-site"
//! inside an unrelated sentence, is correctly left unguarded rather than mis-asserted).
//!
//! DELIBERATE INDEPENDENCE: this file never calls `tests/simplification_audit.rs`'s private
//! `find_heading` / `replace_section_*` / `render_section_6` (integration test binaries cannot
//! see another file's private items anyway) and declares its own minimal cluster shape rather
//! than importing `DupCluster` - the same position a real downstream reader of both committed
//! artifacts is in.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use regex::Regex;

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
/// building block the heading-anchoring test below uses to reason about line-start boundaries
/// without ever calling the producer's own `find_heading`.
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

// ---------------------------------------------------------------------------------------------
// ROUND 6: the one generic whole-report citation guard (see module doc for why this replaced
// five rounds of section-scoped, hand-anchored checks).
// ---------------------------------------------------------------------------------------------

/// Extracts an `"A<sep>B"`-style pair (the report's own shorthand for "one cluster of A units,
/// its companion cluster of B units") sitting between `before` and `after` inside `block`, split
/// on the literal `sep` that actually separates the two numbers in THIS citation's own prose -
/// `"+"` is the only pair separator this file's scanner ever hands it (see `scan_citations`).
/// The sole two-number extractor the generic pass reuses rather than inventing a second one.
fn number_pair_between(block: &str, before: &str, after: &str, sep: &str) -> Option<(u32, u32)> {
    let start = block.find(before)?;
    let tail = &block[start + before.len()..];
    let end = tail.find(after)?;
    let raw = &tail[..end];
    let (a, b) = raw.split_once(sep)?;
    let a = a.trim().parse::<u32>().ok()?;
    let b = b.trim().parse::<u32>().ok()?;
    Some((a, b))
}

/// Every OUTERMOST balanced `( ... )` span in `text`, as `(byte_offset_of_open_paren,
/// inner_content)`. A flat, non-nested `\(([^()]*)\)` scan is not enough here: a citation's own
/// descriptive clause sometimes carries a nested aside (e.g. section 5.5's "`dup-0617`/
/// `dup-0624` (15+5 sites, ... a `(source, expected_tokens_or_clusters)` table candidate)") -
/// under a flat scan the regex engine fails to close the OUTER paren at all (its `[^()]*` body
/// cannot cross the nested `(`) and silently matches only the harmless inner aside instead,
/// dropping the citation's own site/file pair entirely. Tracking paren depth and only emitting a
/// span when depth returns to zero closes that gap without needing to know in advance which
/// citations happen to carry a nested aside.
fn outermost_parens(text: &str) -> Vec<(usize, &str)> {
    let mut depth: u32 = 0;
    let mut start = 0usize;
    let mut out = Vec::new();
    for (idx, ch) in text.char_indices() {
        match ch {
            '(' => {
                if depth == 0 {
                    start = idx;
                }
                depth += 1;
            }
            ')' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    out.push((start, &text[start + 1..idx]));
                }
            }
            _ => {}
        }
    }
    out
}

/// Non-digit filler bytes tolerated between a number and ITS OWN `site`/`file` keyword before
/// giving up on that keyword (see `number_before_bounded`). Generous enough for every real
/// keyword-adjacent citation in this report (the widest, `"46 sqlite \`Connection::open\` call
/// sites"`, needs 33), tight enough to refuse to reach past an unrelated closer number into a
/// distant one - the exact failure mode `number_before_bounded`'s own doc comment walks through.
const MAX_GAP: usize = 40;

/// Non-digit filler bytes a citation's number+keyword pair may sit away from the `(\`dup-NNNN\`)`
/// paren that names it (see `nearest_token_before`). Wider than `MAX_GAP` because the NUMBER and
/// its KEYWORD are always close together (bounded by `MAX_GAP`), but the whole (number, keyword)
/// unit can sit well before the paren that cites it (the widest real case, item 3's "60 raw
/// `/proc`-path string literals scattered across ... with no shared composer (`dup-0124`)",
/// needs about 125).
const WINDOW: usize = 220;

/// Snaps `idx` FORWARD to the nearest UTF-8 char boundary at or after `idx`. `saturating_sub`
/// computes a raw byte offset with no notion of char boundaries, and this report is human prose
/// that can carry any non-ASCII byte (a curly quote, an accented name, an ellipsis, ...)
/// anywhere - a lookback window's start must never land mid-character. Snapping FORWARD rather
/// than backward SHRINKS the lookback window rather than widening it past what the caller
/// computed, so the guard degrades to a shorter (never longer) window instead of panicking (round
/// 7 fix for `adv-u85c4-r6-scanner-panics-on-non-ascii-byte-near-any-paren`, reproduced and
/// confirmed independently by the round-6 adjudicator outside this tree; regression-tested by
/// `nearest_token_before_does_not_panic_when_the_lookback_window_starts_mid_char` and
/// `scan_citations_does_not_panic_when_a_60_byte_lookback_starts_mid_char` below).
fn ceil_char_boundary(text: &str, idx: usize) -> usize {
    let mut i = idx.min(text.len());
    while i < text.len() && !text.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// The nearest bare integer ending at byte `pos` in `block`, skipping only non-digit filler
/// going backward and refusing two ways a naive "walk back to any digit" scan mis-fires when run
/// on whole, un-anchored prose rather than a small hand-picked span:
///
/// 1. UNBOUNDED filler lets an entirely unrelated, more-distant number win. E.g. scanning back
///    from "sites" in `"#### 10. Consolidate the 649 \`.rigger\`-path string-literal sites"`
///    with no gap limit would (correctly) find 649 - but scanning back from "sites" in `"the two
///    named sites of section 2's own catalogued twin duplicate pair"` (a GENERIC use of the word
///    "sites", not a citation at all) would keep skipping non-digit filler across several
///    unrelated clauses until it reached some distant line-number digit, misattributing it.
///    Bounding the filler to `max_gap` makes both cases resolve correctly: the real citation's
///    number is always within a few words of its keyword; a merely-generic use of "sites" with
///    no number of its own within that budget correctly finds nothing.
/// 2. A digit run immediately preceded by `-<digit>` is the SECOND half of an "A-B" hyphenated
///    RANGE ("2-5-site", "a 6-8 files" pair shared by two different cluster ids), not a bound
///    per-id count - returning it as one would silently mis-check an approximate range against
///    an exact catalog count. Rejected outright rather than returned.
fn number_before_bounded(block: &str, pos: usize, max_gap: usize) -> Option<u32> {
    let bytes = block.as_bytes();
    let mut end = pos;
    let mut gap = 0usize;
    while end > 0 && !bytes[end - 1].is_ascii_digit() {
        end -= 1;
        gap += 1;
        if gap > max_gap {
            return None;
        }
    }
    if end == 0 {
        return None;
    }
    let mut start = end;
    while start > 0 && bytes[start - 1].is_ascii_digit() {
        start -= 1;
    }
    if start > 0 && bytes[start - 1] == b'-' {
        return None; // "A-B<kw>" range shape, not a single citation
    }
    block[start..end].parse::<u32>().ok()
}

/// A `site`/`sites`/`file`/`files` keyword occurrence in `block`, as `(byte_start, metric)` -
/// covers the hyphen-attached form (`"15-site"`), the plain word (also matching the literal
/// `"site(s)"`/`"file(s)"` the mandatory-sweep list uses, since `\b` already ends the match right
/// before the parenthesis), and `"string literal(s)"` - the report's own recurring synonym for a
/// site count on all three of its named string-literal sweeps (`dup-0006`, `dup-0051`,
/// `dup-0124`'s own mandatory-sweep headers spell it out: `"... string literals: N site(s)"`).
fn keyword_positions(block: &str) -> Vec<(usize, &'static str)> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"-(?:site|file)s?\b|\b(?:sites?|files?)\b|string\s+literals?").unwrap()
    });
    re.find_iter(block)
        .map(|m| {
            let metric = if m.as_str().contains("file") {
                "file"
            } else {
                "site"
            };
            (m.start(), metric)
        })
        .collect()
}

/// Every `(number, metric, keyword_byte_start)` this file's scanner can mechanically read out of
/// `block` - one call per keyword occurrence, each independently bounded (see
/// `number_before_bounded`), so an unrelated keyword elsewhere in `block` can never steal a
/// number that belongs to a different keyword.
fn tokens_in(block: &str) -> Vec<(u32, &'static str, usize)> {
    keyword_positions(block)
        .into_iter()
        .filter_map(|(kw_start, metric)| {
            number_before_bounded(block, kw_start, MAX_GAP).map(|n| (n, metric, kw_start))
        })
        .collect()
}

/// The markdown site-listing bullet format every mandatory-sweep and cluster site table uses:
/// `` - `path:line-line` `` followed by a backtick-quoted excerpt of the source text AT that
/// location. These excerpts are raw, machine-copied source/doc text, not hand-authored citation
/// prose - and because this report documents its OWN test files (this one included), an excerpt
/// can coincidentally quote a real citation's exact wording verbatim, including its own
/// `(\`dup-NNNN\`)` tag. Recognizing and skipping the bullet LINE itself (never its content)
/// keeps such an excerpt from being read as a second, spurious citation of that id.
fn is_site_listing_line(text: &str, pos: usize) -> bool {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^-\s*`[^`]+:\d+-\d+`").unwrap());
    let line_start = text[..pos].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[pos..].find('\n').map_or(text.len(), |i| pos + i);
    re.is_match(&text[line_start..line_end])
}

/// The nearest `(number, metric)` token appearing anywhere in the up-to-`WINDOW` bytes before
/// byte `paren_start` in `text`, never crossing a preceding `)` (a prior, already-closed
/// citation's own parenthetical) or newline (a preceding, unrelated paragraph) - the same
/// "closest wins, never crosses an unrelated boundary" contract `nearest_token_before_paren`
/// needs whether the id sits bare inside its own parens or right after a paren whose own content
/// carried no token of its own.
fn nearest_token_before(text: &str, paren_start: usize) -> Option<(u32, &'static str)> {
    let lo = ceil_char_boundary(text, paren_start.saturating_sub(WINDOW));
    let mut slice = &text[lo..paren_start];
    if let Some(close) = slice.rfind(')') {
        slice = &slice[close + 1..];
    }
    if let Some(nl) = slice.rfind('\n') {
        slice = &slice[nl + 1..];
    }
    tokens_in(slice)
        .into_iter()
        .max_by_key(|(_, _, kw_start)| *kw_start)
        .map(|(n, m, _)| (n, m))
}

/// One mechanically-found citation: `dup_id` was cited as `cited` `metric`(s) at `line`.
struct Citation {
    line: usize,
    dup_id: String,
    metric: &'static str,
    cited: u32,
}

fn line_of(text: &str, pos: usize) -> usize {
    text[..pos].matches('\n').count() + 1
}

/// THE GENERIC PASS (round 6's whole replacement for rounds 1-5's per-section, hand-anchored
/// checks - see module doc). Finds every `dup-NNNN` id cited ANYWHERE in `report` together with
/// the number(s) cited near it, never typing a single citation's own surrounding prose as an
/// anchor. Three shapes cover every citation convention this report actually uses (verified
/// against the real committed report - see decision `sdet-u85c4-r6-generic-scanner-design`):
///
/// 1. One or two ids sit immediately before an `(...)` - the paren's own content (or, if that
///    content carries no token of its own, e.g. `"(\`dup-0335\`, exact; e.g. ...)"` where the
///    real count sits in the sentence BEFORE the paren, the nearest token before the paren
///    instead) supplies the number(s). TWO ids joined by `/` (`` `dup-0583`/`dup-0585` ``) are
///    only split when the paren's own content opens with an explicit `"A+B <metric>"` pair -
///    section 5.4's `` `dup-0369`/`dup-0368` (\`seed_run_events\`, ..., 6-8 files) `` uses a
///    hyphen, not `+`, because "6-8" is prose describing an approximate RANGE across two
///    different cluster ids, not an exact per-id split (dup-0369 is actually 8, dup-0368 is
///    actually 6 - the reverse of their own textual order) - correctly left unguarded rather
///    than mis-asserting `dup-0369=6, dup-0368=8` in id order.
/// 2. The id sits bare inside its own `(\`dup-NNNN\`)` with no other content - the nearest token
///    before the paren supplies the number (`"the 649 \`.rigger\`-path ... sites (\`dup-0051\`)"`).
/// 3. The id is the very first thing inside the paren, followed by its own count in the SAME
///    paren (`"(\`dup-0125\`, 14 sites: ...)"`) - and, when a SECOND id is named later in that
///    same paren via `` `id`'s N-metric `` (section 5.4's `"...(a companion, 15-file/15-site
///    variant ..., alongside \`dup-0367\`'s 18-file version)"`), tokens before that mention
///    belong to the outer id and the mention's own token belongs to the nested id - never
///    coupling the two into one anchor the way the round-5-rejected mechanism did.
///
/// Plus the mandatory-sweep list's own dash format (`"263 site(s) - \`dup-0006\`"`), which has
/// no parens at all. Machine-generated raw source/doc excerpts (`is_site_listing_line`) are
/// skipped so a self-referential quote of a citation's own wording is never read as a second
/// citation of it.
fn scan_citations(report: &str) -> Vec<Citation> {
    static ID: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static ID_BEFORE_PAREN: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static NESTED_ID_METRIC: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static PAIR: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static SWEEP: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();

    let id_re = ID.get_or_init(|| Regex::new(r"`(dup-\d{4})`").unwrap());
    let id_before_paren_re = ID_BEFORE_PAREN
        .get_or_init(|| Regex::new(r"`(dup-\d{4})`(?:\s*/\s*`(dup-\d{4})`)?\s*$").unwrap());
    let nested_re = NESTED_ID_METRIC
        .get_or_init(|| Regex::new(r"`(dup-\d{4})`'s\s+(\d+)-(site|file)").unwrap());
    let pair_re = PAIR.get_or_init(|| Regex::new(r"^\s*(\d+)\+(\d+)\s+(sites?|files?)").unwrap());
    let sweep_re =
        SWEEP.get_or_init(|| Regex::new(r"(\d+)\s+(site|file)\(s\)\s*-\s*`(dup-\d{4})`").unwrap());

    let mut out = Vec::new();

    for (start, content) in outermost_parens(report) {
        if is_site_listing_line(report, start) {
            continue;
        }
        let before_lo = ceil_char_boundary(report, start.saturating_sub(60));
        let before_text = &report[before_lo..start];

        if let Some(caps) = id_before_paren_re.captures(before_text) {
            let id1 = caps[1].to_string();
            let id2 = caps.get(2).map(|m| m.as_str().to_string());
            let line = line_of(report, start);

            if let Some(id2) = id2 {
                // pair_re only GATES the shape (confirms content opens with an explicit
                // "A+B <metric>" pair, not e.g. a hyphenated range like "6-8 files");
                // number_pair_between - the one shared two-number extractor - does the actual
                // pull, anchored on the keyword pair_re itself just found (so it reads exactly
                // the same span pair_re confirmed, never a coincidental later occurrence).
                if let Some(pm) = pair_re.captures(content) {
                    let keyword = pm[3].to_string();
                    if let Some((a, b)) = number_pair_between(content, "", &keyword, "+") {
                        let metric = if keyword.contains('f') {
                            "file"
                        } else {
                            "site"
                        };
                        out.push(Citation {
                            line,
                            dup_id: id1,
                            metric,
                            cited: a,
                        });
                        out.push(Citation {
                            line,
                            dup_id: id2,
                            metric,
                            cited: b,
                        });
                    }
                }
                // else: not an explicit "A+B <metric>" pair - correctly left unguarded, see
                // this function's own doc comment.
                continue;
            }

            // single id right before this paren
            let nested = nested_re.captures(content);
            let head = match &nested {
                Some(nm) => &content[..nm.get(0).unwrap().start()],
                None => content,
            };
            let mut found_any = false;
            for (num, metric, _) in tokens_in(head) {
                out.push(Citation {
                    line,
                    dup_id: id1.clone(),
                    metric,
                    cited: num,
                });
                found_any = true;
            }
            if let Some(nm) = nested {
                let nested_id = nm[1].to_string();
                let nested_num: u32 = nm[2].parse().unwrap();
                let nested_metric = if &nm[3] == "file" { "file" } else { "site" };
                out.push(Citation {
                    line,
                    dup_id: nested_id,
                    metric: nested_metric,
                    cited: nested_num,
                });
                found_any = true;
            }
            if !found_any {
                if let Some((num, metric)) = nearest_token_before(report, start) {
                    out.push(Citation {
                        line,
                        dup_id: id1,
                        metric,
                        cited: num,
                    });
                }
            }
            continue;
        }

        let content_trim = content.trim();
        if let Some(caps) = id_re.captures(content_trim) {
            if caps.get(0).unwrap().as_str() == content_trim {
                // bare "(`dup-NNNN`)" - the count lives before this paren, not inside it.
                let id1 = caps[1].to_string();
                let line = line_of(report, start);
                if let Some((num, metric)) = nearest_token_before(report, start) {
                    out.push(Citation {
                        line,
                        dup_id: id1,
                        metric,
                        cited: num,
                    });
                }
                continue;
            }
        }

        if let Some(caps) = id_re.captures(content) {
            let m = caps.get(0).unwrap();
            if m.start() == 0 {
                // leading-id paren: id is the first thing inside, possibly with its own count
                // in the same paren ("(`dup-0125`, 14 sites: ...)").
                let id1 = caps[1].to_string();
                let rest = &content[m.end()..];
                let line = line_of(report, start);
                let mut found_any = false;
                for (num, metric, _) in tokens_in(rest) {
                    out.push(Citation {
                        line,
                        dup_id: id1.clone(),
                        metric,
                        cited: num,
                    });
                    found_any = true;
                }
                if !found_any {
                    if let Some((num, metric)) = nearest_token_before(report, start) {
                        out.push(Citation {
                            line,
                            dup_id: id1,
                            metric,
                            cited: num,
                        });
                    }
                }
            }
        }
    }

    for caps in sweep_re.captures_iter(report) {
        let num: u32 = caps[1].parse().unwrap();
        let metric = if &caps[2] == "file" { "file" } else { "site" };
        let dup_id = caps[3].to_string();
        let line = line_of(report, caps.get(0).unwrap().start());
        out.push(Citation {
            line,
            dup_id,
            metric,
            cited: num,
        });
    }

    out
}

/// Appends a mismatch line to `mismatches` when `cited` disagrees with `counts[dup_id]`, or when
/// `dup_id` names no cluster in the committed catalog at all - a citation of an id that does not
/// exist is exactly as much a boundary defect as a wrong count, so it is collected here rather
/// than aborting the whole scan on the first such id (every OTHER citation still gets checked).
fn record_mismatch(
    mismatches: &mut Vec<String>,
    location: &str,
    dup_id: &str,
    metric_name: &str,
    cited: u32,
    counts: &HashMap<String, usize>,
) {
    match counts.get(dup_id) {
        None => mismatches.push(format!(
            "{location}: {dup_id} is cited but has no cluster in {CATALOG_PATH}"
        )),
        Some(&actual) if actual as u32 != cited => mismatches.push(format!(
            "{location}: {dup_id} cited as {cited} {metric_name}(s) in {REPORT_PATH}, but \
             {CATALOG_PATH} carries {actual} {metric_name}(s)"
        )),
        Some(_) => {}
    }
}

/// THE CROSS-ARTIFACT CONTRACT (round 6, replacing rounds 1-5's `section_5_named_dup_id_
/// citations_match_the_committed_catalogs_site_counts` and `section_6_named_dup_id_citations_
/// match_the_committed_catalogs_site_counts` - see module doc): every `dup-NNNN` cluster citation
/// mechanically found anywhere in `docs/audit/2026-09-simplification-audit.md` by `scan_citations`
/// is checked against the committed `docs/audit/duplication-catalog.json`, independent of which
/// section it sits in or whoever authored that prose.
#[test]
fn every_dup_id_citation_anywhere_in_the_report_matches_the_committed_catalog() {
    let report = read_report();
    let sites = catalog_site_counts();
    let files = catalog_file_counts();

    let citations = scan_citations(&report);
    assert!(
        citations.len() > 600,
        "sanity: the generic scanner found only {} citations across the whole report - expected \
         well over 600 (674 catalog clusters plus section 5/6's own narrative citations); this \
         smells like the scanner itself broke, not that the report suddenly has far fewer \
         citations",
        citations.len()
    );

    let mut mismatches = Vec::new();
    for c in &citations {
        let counts = if c.metric == "file" { &files } else { &sites };
        record_mismatch(
            &mut mismatches,
            &format!("line {}", c.line),
            &c.dup_id,
            c.metric,
            c.cited,
            counts,
        );
    }

    assert!(
        mismatches.is_empty(),
        "{REPORT_PATH} cites stale site/file counts that no longer match the committed \
         duplication catalog:\n{}",
        mismatches.join("\n")
    );
}

// ---------------------------------------------------------------------------------------------
// ROUND 7 REGRESSION: the lookback slices above are raw byte-index slices with no char-boundary
// check - a multi-byte UTF-8 character landing inside a lookback window used to panic with
// "byte index N is not a char boundary" (adjudicator `adv-u85c4-r6-scanner-panics-on-non-ascii-
// byte-near-any-paren`, reproduced independently outside this tree and confirmed live). Both
// tests below construct text where the naive `saturating_sub` lookback start lands on the SECOND
// byte of a 3-byte UTF-8 character (`'\u{2026}'`, an ellipsis) - never on a char boundary - so a
// fix that merely avoids panicking BY LUCK on today's all-ASCII report would still fail these.
// ---------------------------------------------------------------------------------------------

/// `nearest_token_before`'s own `WINDOW`-byte lookback (periphery.rs's own `WINDOW` constant)
/// must not panic when its computed start lands mid-character - it should degrade to a shorter
/// window instead, exactly as the round-6 adjudication's remedy names it.
#[test]
fn nearest_token_before_does_not_panic_when_the_lookback_window_starts_mid_char() {
    // Byte layout: 19 ASCII bytes, then a 3-byte char at [19..22), then `WINDOW - 2` more ASCII
    // bytes, then the paren. `paren_start` = 19 + 3 + (WINDOW - 2) = WINDOW + 20, which exceeds
    // `WINDOW` so `saturating_sub` does not clamp to 0; the resulting `WINDOW`-byte lookback
    // lands at byte 20 - the SECOND byte of the 3-byte char (not a boundary) - by construction,
    // not by luck.
    let prefix = "a".repeat(19);
    let pad = "b".repeat(WINDOW - 2);
    let text = format!("{prefix}\u{2026}{pad}(");
    let paren_start = text.len() - 1;
    assert_eq!(&text[paren_start..], "(");
    // Must not panic - the real defect being regression-tested.
    let _ = nearest_token_before(&text, paren_start);
}

/// `scan_citations`'s own fixed 60-byte lookback (periphery.rs:414-415) must not panic when its
/// computed start lands mid-character, for the same reason and the same construction as above
/// (scaled to the 60-byte window instead of `WINDOW`).
#[test]
fn scan_citations_does_not_panic_when_a_60_byte_lookback_starts_mid_char() {
    // Byte layout: 19 ASCII bytes, then a 3-byte char at [19..22), then 58 more ASCII bytes,
    // then an outermost `(...)`. `start` (the paren's own byte offset) = 80; the 60-byte lookback
    // lands at byte 20 - the SECOND byte of the 3-byte char (not a boundary).
    let prefix = "a".repeat(19);
    let pad = "b".repeat(58);
    let text = format!("{prefix}\u{2026}{pad}(`dup-0001`)");
    // Must not panic - the real defect being regression-tested.
    let _ = scan_citations(&text);
}
