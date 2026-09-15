//! Spec 85 criterion 1 (u85c1), SDET periphery layer: a serialized-form contract test for
//! `docs/audit/responsibility-map.json`.
//!
//! Boundary-surface accounting (mechanical probes against base
//! `f31f3adb16d67853031daa07cd68db23da4bb72b`, see decision `sdet-u85c1-surface-accounting`):
//! new/changed `pub` API, trait impl, CLI surface, and cross-module seam all came back empty
//! (this unit changes no `src/` file at all). The one item the "event type / serialized form"
//! probe surfaced is `tests/simplification_audit.rs`'s private `struct MapEntry`, which derives
//! BOTH `Serialize` and `Deserialize`. Spec 85's Design says the JSON it produces is
//! "machine-readable so follow-up specs can pin counts and prove reductions" - `Deserialize`
//! exists specifically so a LATER, AS-YET-UNWRITTEN consumer (u85c2/u85c3/u85c4, or a refactor
//! spec from section 6 reading this artifact to pin a baseline count) can read the file back.
//! Yet nothing in that unit's own 28 inline `#[test]`s ever exercises the `Deserialize` half:
//! every one of them calls `build_map`/`map_to_json`/`render_section_1` in-process and compares
//! freshly-computed or committed STRINGS - none of them ever round-trip the persisted JSON back
//! through `serde_json` the way a downstream reader will. That is the boundary this file tests.
//!
//! DELIBERATE INDEPENDENCE: this file declares its OWN `ConsumedMapEntry` struct rather than
//! importing the producer's `MapEntry` - not just because Cargo integration-test binaries each
//! compile as a separate crate and cannot see another test file's private items anyway, but
//! because a real downstream consumer is in exactly this position: holding only the committed
//! JSON file and spec 85's documented shape, blind to the producer's internal Rust type. Field
//! order below matches `MapEntry`'s declaration order (`tests/simplification_audit.rs:1220`)
//! exactly, which is also why the round-trip test below can assert byte-identical re-encoding.
//!
//! This unit does NOT own the duplication catalog, report sections 2-6, or any production code
//! (spec 85: "no production code changes"), so this file drives no binary and spawns no
//! process - the whole surface to prove is the persisted data contract itself.
//!
//! SPEC 90 CRITERION 2 ACCOUNTING (decision `sdet-u90c2-surface-accounting`, correction
//! `sdet-u90c2-claim1-subsumed-by-roundtrip`): CLAIM 1 needs no new test - the pre-existing
//! `deserializing_then_reserializing_reproduces_the_committed_bytes_exactly` below already
//! proves it strictly. Two genuine gaps closed here: (1)
//! `docs/audit/responsibility-map.lines.json`, spec 90's new unguarded sibling, had ZERO test
//! coverage - `ConsumedMapEntryLines`/`deserialize_committed_map_lines` plus
//! `the_committed_map_and_its_lines_sibling_are_position_joined` close it (`MAP_PATH`/
//! `MAP_LINES_PATH` are built from the SAME `Vec<MapEntry>` with no resort in either
//! `map_to_json` or `map_lines_to_json` - array position is a valid join here; unlike
//! `tests/dead_code_json_contract_periphery.rs`'s equivalent, this does not also assert a global
//! ascending order - see that test's own doc comment for why). (2) CLAIM 4 had
//! NO test anywhere before this unit for this artifact -
//! `the_committed_report_section_1_cites_file_line_exactly_as_the_lines_sibling_records_them`
//! below closes it, reading the COMMITTED report and COMMITTED lines sibling directly. It joins
//! by `(file, start_line)`, never `(file, name)` or bare array position: `render_section_1`
//! re-groups entries by PROPOSED MODULE (sorted alphabetically), so the report's own citation
//! order is a permutation of `MAP_LINES_PATH`'s array order, ruling out a positional join;
//! `(file, name)` is not unique either - `src/conductor.rs`'s three `Error::from` impls all share
//! one bare name in one file (visible in the committed map's own `conductor::error` module
//! group). `(file, start_line)` is the one key that is: no two function definitions can start on
//! the same line of the same file.

use serde::Deserialize;
use std::path::PathBuf;

/// Mirrors `tests/simplification_audit.rs`'s private `MapEntryWire` shape field-for-field, from
/// the outside - see the module doc comment for why this is a deliberate re-declaration, not an
/// import. Spec 90 criterion 2: the guarded map carries `content_hash`, never a line span -
/// `start_line`/`end_line` moved to the unguarded `docs/audit/responsibility-map.lines.json`
/// sibling, out of scope for this file (never drift-guarded, so not a "documented contract" a
/// downstream reader pins against).
#[derive(Debug, Clone, PartialEq, Deserialize, serde::Serialize)]
struct ConsumedMapEntry {
    file: String,
    name: String,
    is_test: bool,
    proposed_module: Option<String>,
    reason: String,
    content_hash: String,
}

const MAP_PATH: &str = "docs/audit/responsibility-map.json";
const TARGET_FILES: [&str; 3] = ["src/conductor.rs", "src/main.rs", "src/dash.rs"];

/// The repo root this test binary was compiled from - never the process CWD (same convention
/// as `tests/simplification_audit.rs::repo_root` and `tests/no_os_kill_audit.rs`).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_committed_map_raw() -> String {
    let path = repo_root().join(MAP_PATH);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{MAP_PATH} is missing or unreadable ({e})"))
}

fn deserialize_committed_map() -> Vec<ConsumedMapEntry> {
    let raw = read_committed_map_raw();
    serde_json::from_str(&raw).unwrap_or_else(|e| {
        panic!(
            "{MAP_PATH} does not deserialize as the documented MapEntry contract \
             (file/name/is_test/proposed_module/reason/content_hash): {e}"
        )
    })
}

/// THE ROUND-TRIP PROOF: a downstream consumer who only has spec 85's documented field shape
/// (not the producer's private Rust type) can actually parse the committed artifact. This is
/// the specific gap the boundary probe found - `Deserialize` is derived but never exercised
/// anywhere in the unit's own tests.
#[test]
fn the_committed_responsibility_map_deserializes_as_a_downstream_consumer_would() {
    let entries = deserialize_committed_map();
    assert!(
        !entries.is_empty(),
        "{MAP_PATH} deserialized to zero entries - a downstream consumer pinning counts \
         against this file would silently see nothing"
    );
}

/// Every entry names one of the three files spec 85's Done-when criterion 1 fixes by literal
/// path (`src/conductor.rs`, `src/main.rs`, `src/dash.rs`) - a consumer filtering by file (e.g.
/// a later refactor spec pinning `conductor.rs`'s own count) must never see a stray value.
#[test]
fn every_deserialized_entry_names_one_of_the_three_target_files() {
    let entries = deserialize_committed_map();
    for e in &entries {
        assert!(
            TARGET_FILES.contains(&e.file.as_str()),
            "entry {:?} names file {:?}, not one of {TARGET_FILES:?}",
            e.name,
            e.file
        );
    }
}

/// Every entry's identity is one a reader can act on: a non-empty `content_hash` - the LINE-FREE
/// contract spec 90 criterion 2 establishes (no `start_line`/`end_line` at all: an extra field
/// would silently fail to deserialize into this independently-declared struct, making the
/// round-trip test below the real proof of their absence).
#[test]
fn every_deserialized_entry_has_a_non_empty_content_hash() {
    let entries = deserialize_committed_map();
    for e in &entries {
        assert!(
            !e.content_hash.is_empty(),
            "entry {:?} ({}) has an empty content_hash",
            e.name,
            e.file
        );
    }
}

/// Spec 85: "unassignable functions are named as such, never omitted." The producer's own test
/// checks this against `build_map`'s in-memory output; this checks it against what is actually
/// on disk and read back through the documented JSON contract, closing the gap between "the
/// generator computed the right thing" and "the persisted artifact still says the right thing."
#[test]
fn unassigned_entries_in_the_committed_map_still_name_their_function() {
    let entries = deserialize_committed_map();
    let unassigned: Vec<&ConsumedMapEntry> = entries
        .iter()
        .filter(|e| e.proposed_module.is_none())
        .collect();
    assert!(
        !unassigned.is_empty(),
        "expected at least one honestly-unassigned entry in the real tree (decision \
         u85c1-classification-scheme records 163)"
    );
    for e in &unassigned {
        assert!(
            !e.name.is_empty(),
            "unassigned entry in {} has an empty name",
            e.file
        );
        assert!(
            !e.file.is_empty(),
            "unassigned entry {:?} has an empty file",
            e.name
        );
        assert!(
            !e.reason.is_empty(),
            "unassigned entry {:?} ({}) carries no reason - spec 85 requires unassignable \
             functions to be NAMED, not silently dropped",
            e.name,
            e.file
        );
    }
}

/// THE is_test / proposed_module CONSISTENCY PROOF, round 2 addendum (decision
/// `sdet-u85c1-r2-surface-accounting`): the adjudicator's round-1 REJECT
/// (`adj-u85c1-verdict-reject-impl-frame-swallows-test-ancestry`) was exactly a violation of
/// this cross-field invariant - 70 entries committed with `is_test:false` under a fabricated
/// PRODUCTION module while genuinely living inside test-only impl blocks. The round-2 fix
/// (decision `u85c1-r2-fix-impl-test-ancestry`) restored it, verified ad hoc against the
/// committed file at fix time - but nothing PINS it going forward. The generator's own
/// drift-guard test only proves the committed JSON matches whatever `build_map` computes
/// TODAY (self-consistency); it does not, and structurally cannot, prove that computation is
/// correct. A future edit to `classify()` that reintroduces this exact class of bug (or its
/// mirror - a genuinely-test fn losing its `is_test` flag) would pass every one of the
/// producer's own tests and the drift guard unchanged, and only a periphery test reading the
/// artifact from the outside catches it. Per decision `u85c1-classification-scheme`, a
/// `is_test:true` row is ALWAYS assigned (never left unassigned) to `<file_stem>::tests` or a
/// nested `<file_stem>::tests::<submodule>`; a `is_test:false` row is NEVER assigned into a
/// `::tests` module. Verified against the real committed file (1493 entries) before writing
/// this test: 0 violations either direction.
#[test]
fn is_test_rows_and_only_is_test_rows_land_in_a_tests_proposed_module() {
    let entries = deserialize_committed_map();
    for e in &entries {
        if e.is_test {
            let module = e.proposed_module.as_deref().unwrap_or_else(|| {
                panic!(
                    "entry {:?} ({}) is is_test:true but has no proposed_module - decision \
                     u85c1-classification-scheme says is_test always wins assignment, never \
                     leaves a test fn unassigned",
                    e.name, e.file
                )
            });
            assert!(
                module.contains("::tests"),
                "entry {:?} ({}) is is_test:true but proposed_module {module:?} does not \
                 contain \"::tests\" - this is precisely the class of bug the adjudicator's \
                 round-1 REJECT (adj-u85c1-verdict-reject-impl-frame-swallows-test-ancestry) \
                 found: a test-only fn given a production module home",
                e.name,
                e.file
            );
        } else if let Some(module) = &e.proposed_module {
            assert!(
                !module.contains("::tests"),
                "entry {:?} ({}) is is_test:false but proposed_module {module:?} contains \
                 \"::tests\" - a genuinely-production fn must never be homed under a test \
                 module",
                e.name,
                e.file
            );
        }
    }
}

/// THE BACK-COMPAT / STABILITY PROOF: deserializing the committed file into this independently
/// declared struct and re-serializing it (same field order, `serde_json::to_string_pretty` plus
/// the producer's own trailing-newline convention) reproduces the committed bytes exactly. This
/// is the strongest form of the round-trip contract - it proves the JSON shape is lossless and
/// canonical from an outside reader's perspective, not merely that the producer's own function
/// agrees with itself (every one of the implementer's own drift-guard tests compares the SAME
/// producer type/function on both sides; this test decodes and re-encodes through a
/// SEPARATELY-declared type, the position any real future consumer will be in).
#[test]
fn deserializing_then_reserializing_reproduces_the_committed_bytes_exactly() {
    let committed = read_committed_map_raw();
    let entries = deserialize_committed_map();
    let mut reencoded =
        serde_json::to_string_pretty(&entries).expect("ConsumedMapEntry re-serializes");
    reencoded.push('\n');
    assert_eq!(
        committed, reencoded,
        "{MAP_PATH} does not round-trip byte-for-byte through the documented MapEntry shape - \
         a downstream consumer decoding and re-encoding this file would silently diverge from \
         the committed artifact"
    );
}

// -----------------------------------------------------------------------------------------
// Spec 90 criterion 2, THE DRIFT GUARD IS LINE-FREE: the unguarded `.lines.json` sibling
// (zero prior coverage) and CLAIM 4 at the true periphery level.
// -----------------------------------------------------------------------------------------

/// Spec 90 criterion 2: the UNGUARDED sibling carrying [`MAP_PATH`]'s line spans, joined to it
/// by array POSITION (never drift-guarded, so not itself a "documented contract" - read here
/// only to prove the join and the ordering it preserves, same convention as
/// `tests/dead_code_json_contract_periphery.rs`'s own `DEAD_CODE_LINES_PATH`).
const MAP_LINES_PATH: &str = "docs/audit/responsibility-map.lines.json";

/// Mirrors `tests/simplification_audit.rs`'s private `MapEntryLines` shape field-for-field.
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct ConsumedMapEntryLines {
    file: String,
    name: String,
    start_line: usize,
    end_line: usize,
}

fn deserialize_committed_map_lines() -> Vec<ConsumedMapEntryLines> {
    let path = repo_root().join(MAP_LINES_PATH);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{MAP_LINES_PATH} is missing or unreadable ({e})"));
    serde_json::from_str(&raw).unwrap_or_else(|e| {
        panic!("{MAP_LINES_PATH} does not deserialize as the documented lines contract: {e}")
    })
}

/// The join, checked against the PERSISTED files rather than the generator's in-memory value:
/// same length, same `(file, name)` at every index. Unlike
/// `tests/dead_code_json_contract_periphery.rs`'s equivalent test, this does NOT also assert a
/// global ascending-by-`(file, start_line)` order: `build_map`'s scanner visits an out-of-line
/// `#[cfg(test)] mod tests` body as a distinct pass after its enclosing file's top-level scan
/// (verified against the real committed file: the first non-monotonic `start_line` step, index
/// 241, is exactly such a seam), so entries are grouped by scan pass, not globally
/// line-sorted - a real, deliberate property of the scanner, not a drift-guard gap this unit
/// introduces or this test should assert against.
#[test]
fn the_committed_map_and_its_lines_sibling_are_position_joined() {
    let entries = deserialize_committed_map();
    let lines = deserialize_committed_map_lines();
    assert_eq!(
        entries.len(),
        lines.len(),
        "{MAP_PATH} and {MAP_LINES_PATH} must have equal length - they are joined by array \
         position"
    );
    for (i, (e, l)) in entries.iter().zip(lines.iter()).enumerate() {
        assert_eq!(
            (&e.file, &e.name),
            (&l.file, &l.name),
            "{MAP_PATH} and {MAP_LINES_PATH} entry {i} disagree on (file, name) - the two files \
             are joined by array position and must describe the same entry at each index"
        );
    }
}

const REPORT_PATH: &str = "docs/audit/2026-09-simplification-audit.md";

/// Section 1's per-entry citations (`render_section_1`'s own shared template for both the
/// "Proposed module tree" and "Unassigned" lists: `` `{file}:{start}-{end}` `{name}` - {reason}
/// ``), extracted from the section between its own "## 1." heading and the next "## 2." heading.
fn section_1_citations(report: &str) -> Vec<(String, String, usize, usize)> {
    let start = report
        .find("## 1. Responsibility Map")
        .expect("report has a 1. heading");
    let rest = &report[start..];
    let end = rest
        .find("\n## 2. ")
        .map(|i| i + start)
        .unwrap_or(report.len());
    let section = &report[start..end];
    let re = regex::Regex::new(r"(?m)^\s*- `([^`]+):(\d+)-(\d+)` `([^`]+)`").expect("valid regex");
    re.captures_iter(section)
        .map(|c| {
            (
                c[4].to_string(),
                c[1].to_string(),
                c[2].parse().expect("digits"),
                c[3].parse().expect("digits"),
            )
        })
        .collect()
}

/// CLAIM 4: "the report still cites `file:line` from the unguarded lines file." Every citation
/// in section 1 of the COMMITTED report (both the module tree and the unassigned list share one
/// template) matches the COMMITTED `MAP_LINES_PATH` entry with the SAME `(file, start_line)` -
/// the one join key guaranteed unique here (no two functions start on the same line of the same
/// file; `(file, name)` is NOT unique - see the module doc comment's `Error::from` example, and
/// `render_section_1` re-groups by module so bare array position does not correspond either).
#[test]
fn the_committed_report_section_1_cites_file_line_exactly_as_the_lines_sibling_records_them() {
    let report = std::fs::read_to_string(repo_root().join(REPORT_PATH))
        .unwrap_or_else(|e| panic!("{REPORT_PATH} is missing or unreadable ({e})"));
    let citations = section_1_citations(&report);
    let lines = deserialize_committed_map_lines();
    assert_eq!(
        citations.len(),
        lines.len(),
        "section 1 of {REPORT_PATH} renders {} citation(s) but {MAP_LINES_PATH} records {} - \
         every entry must appear exactly once (module tree + unassigned lists partition the \
         full set)",
        citations.len(),
        lines.len()
    );
    for (name, file, start, end) in &citations {
        let entry = lines
            .iter()
            .find(|l| &l.file == file && l.start_line == *start)
            .unwrap_or_else(|| {
                panic!(
                    "report cites {name} ({file}:{start}-{end}) in section 1, but no entry with \
                     that (file, start_line) exists in {MAP_LINES_PATH}"
                )
            });
        assert_eq!(
            &entry.name, name,
            "report section 1 cites {file}:{start}-{end} as {name:?}, but {MAP_LINES_PATH} \
             records {:?} for that (file, start_line)",
            entry.name
        );
        assert_eq!(
            entry.end_line, *end,
            "report section 1 cites {name} as {file}:{start}-{end}, but {MAP_LINES_PATH} \
             records end_line {} for that (file, start_line)",
            entry.end_line
        );
    }
}
