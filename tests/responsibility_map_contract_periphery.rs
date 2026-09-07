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

use serde::Deserialize;
use std::path::PathBuf;

/// Mirrors `tests/simplification_audit.rs`'s private `MapEntry` shape field-for-field, from the
/// outside - see the module doc comment for why this is a deliberate re-declaration, not an
/// import.
#[derive(Debug, Clone, PartialEq, Deserialize, serde::Serialize)]
struct ConsumedMapEntry {
    file: String,
    name: String,
    start_line: usize,
    end_line: usize,
    is_test: bool,
    proposed_module: Option<String>,
    reason: String,
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
             (file/name/start_line/end_line/is_test/proposed_module/reason): {e}"
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

/// Every entry's line span is a span a reader can act on: 1-based and non-inverted. A consumer
/// that opens `file` at `start_line` to `end_line` (e.g. to quote the function for a refactor
/// spec stub) must never be handed a zero or backwards range.
#[test]
fn every_deserialized_entry_has_a_well_formed_line_span() {
    let entries = deserialize_committed_map();
    for e in &entries {
        assert!(
            e.start_line >= 1,
            "entry {:?} ({}) has start_line {} < 1",
            e.name,
            e.file,
            e.start_line
        );
        assert!(
            e.end_line >= e.start_line,
            "entry {:?} ({}) has end_line {} < start_line {}",
            e.name,
            e.file,
            e.end_line,
            e.start_line
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
