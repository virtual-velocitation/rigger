//! Spec 87 criterion 2 (u87c2), SDET periphery layer: a serialized-form contract test for
//! `docs/audit/dead-code.json`.
//!
//! Boundary-surface accounting (mechanical probes against base
//! `aa04e72a2739772ef867f375eb72b449803e5161`, see decision `sdet-u87c2-surface-accounting`):
//! new/changed `pub` API, trait impl, and CLI surface all come back empty (this unit changes no
//! `src/` file at all - the whole diff lives inside `tests/simplification_audit.rs`, a single
//! self-contained integration-test binary with no call into `src/`, no new process spawn, and no
//! shell-out to `rigger graph`). The one item the "event type / serialized form" probe surfaces
//! is two private structs, `TestOnlyRef` and `DeadCodeCandidate`, which both derive BOTH
//! `Serialize` and `Deserialize`. Spec 87's OUTPUT section commits this JSON to disk precisely so
//! a later unit (criterion 3, dispositions + report section 4) can read it back - same intent,
//! and the same gap, as spec 85 criterion 2's `duplication-catalog.json` (see
//! `tests/duplication_catalog_contract_periphery.rs`, the direct precedent this file mirrors):
//! nothing in the unit's own tests ever deserializes the ACTUAL COMMITTED FILE through an
//! INDEPENDENTLY-declared struct. The unit's own two related tests are both self-consistency in
//! one way or another -
//! `dead_code_json_matches_the_tree_or_is_rewritten` string-compares the committed bytes against
//! `dead_code_to_json(real_dead_code_candidates())` (the SAME producer function on both sides),
//! and `every_real_candidate_has_a_zero_degree_knowledge_graph_cross_check_shape` asserts
//! `file.starts_with("src/")` against the freshly-computed in-memory value, never the persisted
//! one. Neither is the position a real downstream consumer (criterion 3's future report
//! generator, reading only spec 87's documented shape, blind to this producer's internal Rust
//! type) is actually in. That is the boundary this file tests.
//!
//! DELIBERATE INDEPENDENCE: this file declares its OWN `ConsumedDeadCodeCandidate`/
//! `ConsumedTestOnlyRef` structs rather than importing the producer's `DeadCodeCandidate`/
//! `TestOnlyRef` - not just because Cargo integration-test binaries each compile as a separate
//! crate and cannot see another test file's private items anyway, but because a real downstream
//! consumer is in exactly this position. Field order below matches the producer's declaration
//! order (`tests/simplification_audit.rs`'s `TestOnlyRef`/`DeadCodeCandidate` structs) exactly,
//! which is also why the round-trip test below can assert byte-identical re-encoding; that same
//! byte-identical round-trip is also the mechanical proof that no field beyond the six named here
//! (in particular no `disposition`, which decision `u87c2-json-schema-excludes-disposition`
//! scopes to criterion 3, NOT this one) is present in the real committed file today - an extra
//! field would deserialize-drop and then fail the re-encode comparison below, so a dedicated
//! "no disposition field" test would only duplicate that coverage.
//!
//! This unit does NOT own dispositions, the knowledge-graph degree cross-check, or report
//! section 4 (spec 87 Done-when: "criterion 3, NOT this one's"), so this file drives no binary
//! and spawns no process - the whole surface to prove is the persisted data contract itself.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Mirrors `tests/simplification_audit.rs`'s private `TestOnlyRef` shape field-for-field, from
/// the outside - see the module doc comment for why this is a deliberate re-declaration, not an
/// import.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
struct ConsumedTestOnlyRef {
    file: String,
    line: usize,
}

/// Mirrors `tests/simplification_audit.rs`'s private `DeadCodeCandidate` shape field-for-field.
/// Deliberately has no `disposition` field - see the module doc comment.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
struct ConsumedDeadCodeCandidate {
    name: String,
    file: String,
    line: usize,
    visibility: String,
    ambiguous: bool,
    test_only_references: Vec<ConsumedTestOnlyRef>,
}

const DEAD_CODE_PATH: &str = "docs/audit/dead-code.json";

/// The repo root this test binary was compiled from - never the process CWD (same convention as
/// `tests/simplification_audit.rs::repo_root` and `tests/duplication_catalog_contract_periphery.rs`).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_committed_dead_code_raw() -> String {
    let path = repo_root().join(DEAD_CODE_PATH);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{DEAD_CODE_PATH} is missing or unreadable ({e})"))
}

fn deserialize_committed_dead_code() -> Vec<ConsumedDeadCodeCandidate> {
    let raw = read_committed_dead_code_raw();
    serde_json::from_str(&raw).unwrap_or_else(|e| {
        panic!(
            "{DEAD_CODE_PATH} does not deserialize as the documented DeadCodeCandidate contract \
             (name/file/line/visibility/ambiguous/test_only_references, test_only_references as \
             file/line): {e}"
        )
    })
}

/// THE ROUND-TRIP PROOF: a downstream consumer who only has spec 87's documented field shape
/// (not the producer's private Rust type) can actually parse the committed artifact. This is the
/// specific gap the boundary probe found - `Deserialize` is derived but the unit's own tests
/// never exercise it against the real committed file, only a producer-internal string compare.
#[test]
fn the_committed_dead_code_json_deserializes_as_a_downstream_consumer_would() {
    let candidates = deserialize_committed_dead_code();
    assert!(
        !candidates.is_empty(),
        "{DEAD_CODE_PATH} deserialized to zero candidates - a downstream consumer (criterion 3's \
         report generator) reading this file would silently see nothing to disposition"
    );
}

/// Spec 87 OUTPUT: "one entry per production fn ... name, file:line, visibility, the test-only
/// references". A consumer reading an entry to render section 4 or schedule a deletion must
/// never see a blank name, a file outside `src/` (this criterion's whole point is PRODUCTION
/// fns only - checked here against the PERSISTED file, not `real_dead_code_candidates()`'s
/// in-memory value the way the unit's own
/// `every_real_candidate_has_a_zero_degree_knowledge_graph_cross_check_shape` does), a zero or
/// negative line, or an unrecognized visibility qualifier.
#[test]
fn every_deserialized_candidate_has_a_non_empty_name_a_src_file_a_valid_line_and_a_recognized_visibility(
) {
    let candidates = deserialize_committed_dead_code();
    for c in &candidates {
        assert!(!c.name.is_empty(), "{c:?} has an empty name");
        assert!(
            c.file.starts_with("src/"),
            "{c:?} has a file outside src/ - this criterion's whole point is production fns only"
        );
        assert!(c.line >= 1, "{c:?} has line {} < 1", c.line);
        assert!(
            c.visibility == "private" || c.visibility.starts_with("pub"),
            "{c:?} has an unrecognized visibility {:?}",
            c.visibility
        );
    }
}

/// Every test-only reference is a span a reader can act on: a non-empty file and a 1-based line.
/// A consumer that opens `file` at `line` (e.g. to quote the reference in a disposition's cited
/// reason) must never be handed a blank-named or zero-line reference. Spec 87 Constraints Walk
/// permits an entry with ZERO test-only references (a fn referenced nowhere at all, not even
/// from a test) so this deliberately asserts nothing about count, only per-reference shape.
#[test]
fn every_deserialized_test_only_reference_has_a_non_empty_file_and_a_valid_line() {
    let candidates = deserialize_committed_dead_code();
    for c in &candidates {
        for r in &c.test_only_references {
            assert!(
                !r.file.is_empty(),
                "{} has a test_only_reference with an empty file",
                c.name
            );
            assert!(
                r.line >= 1,
                "{} has a test_only_reference in {} with line {} < 1",
                c.name,
                r.file,
                r.line
            );
        }
    }
}

/// Determinism/ordering, checked against the PERSISTED file rather than the generator's
/// in-memory value (the producer's own `build_dead_code_candidates` sorts its output by
/// `(file, line)` three times over - `tests/simplification_audit.rs`'s internal fixture tests
/// only ever assert individual entries' presence or absence, never that the committed ordering
/// itself is stable). A consumer that iterates the file expecting per-file grouping (e.g. to
/// render section 4's per-file distribution in file order) depends on this ordering surviving
/// the trip to disk, not merely holding in memory at generation time.
#[test]
fn the_committed_dead_code_json_is_sorted_ascending_by_file_then_line() {
    let candidates = deserialize_committed_dead_code();
    let mut sorted = candidates.clone();
    sorted.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    for (i, (actual, expected)) in candidates.iter().zip(sorted.iter()).enumerate() {
        assert_eq!(
            (&actual.file, actual.line),
            (&expected.file, expected.line),
            "{DEAD_CODE_PATH} entry {i} ({}:{}) is out of (file, line) order",
            actual.file,
            actual.line
        );
    }
    for c in &candidates {
        let mut refs_sorted = c.test_only_references.clone();
        refs_sorted.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
        assert_eq!(
            c.test_only_references, refs_sorted,
            "{}'s test_only_references are out of (file, line) order",
            c.name
        );
    }
}

/// THE BACK-COMPAT / STABILITY PROOF: deserializing the committed file into this independently
/// declared struct and re-serializing it (same field order, `serde_json::to_string_pretty` plus
/// the producer's own trailing-newline convention, per `dead_code_to_json`) reproduces the
/// committed bytes exactly. This is the strongest form of the round-trip contract - it proves the
/// JSON shape is lossless and canonical from an outside reader's perspective, not merely that the
/// producer's own function agrees with itself (the implementer's own drift-guard test compares
/// the SAME producer type/function on both sides; this test decodes and re-encodes through a
/// SEPARATELY-declared type, the position any real future consumer will be in). It is also the
/// mechanical proof that no field beyond the six declared above - in particular no
/// `disposition` - is present in the file today: an extra field would silently drop on decode and
/// then fail this exact comparison.
#[test]
fn deserializing_then_reserializing_the_committed_dead_code_json_reproduces_the_committed_bytes_exactly(
) {
    let committed = read_committed_dead_code_raw();
    let candidates = deserialize_committed_dead_code();
    let mut reencoded =
        serde_json::to_string_pretty(&candidates).expect("ConsumedDeadCodeCandidate re-serializes");
    reencoded.push('\n');
    assert_eq!(
        committed, reencoded,
        "{DEAD_CODE_PATH} does not round-trip byte-for-byte through the documented \
         DeadCodeCandidate shape - a downstream consumer decoding and re-encoding this file \
         would silently diverge from the committed artifact"
    );
}
