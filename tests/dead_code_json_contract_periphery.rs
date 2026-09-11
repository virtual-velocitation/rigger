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
//! byte-identical round-trip is also the mechanical proof that no field beyond the seven named
//! here (in particular no `disposition`, which decision `u87c2-json-schema-excludes-disposition`
//! scopes to criterion 3, NOT this one) is present in the real committed file today - an extra
//! field would deserialize-drop and then fail the re-encode comparison below, so a dedicated
//! "no disposition field" test would only duplicate that coverage. `ambiguous_with` (round 1,
//! `op-u87c2-round-1-ambiguity-covers-free-fns-too`) carries the SAME
//! `#[serde(default, skip_serializing_if = "Vec::is_empty")]` shape the producer declares it
//! with, so a non-ambiguous entry (the overwhelming majority) round-trips with no key for it at
//! all - proven by the same byte-identical comparison, not asserted separately.
//!
//! This unit does NOT own dispositions, the knowledge-graph degree cross-check, or report
//! section 4 (spec 87 Done-when: "criterion 3, NOT this one's"), so this file drives no binary
//! and spawns no process - the whole surface to prove is the persisted data contract itself.
//!
//! ROUND 1 ACCOUNTING (decision `sdet-u87c2-r1-surface-accounting`, superseding
//! `sdet-u87c2-surface-accounting` above): round 1's fix
//! (`op-u87c2-round-1-closes-the-reference-classes-not-the-instances`) added a genuine NEW
//! cross-module seam - a call from `tests/simplification_audit.rs` into the real production
//! public API (`rigger::grounder::symbols::build_index` -> `events::index_events`) to prove the
//! bespoke out-of-line-test-file resolver agrees with the canonical production one. The
//! implementer's own 5 new tests already integration-test that PARITY property, on fixtures and
//! on the real tree. What none of them pin is the COMMITTED ARTIFACT itself: a future edit to the
//! generator's call site, or a stale regeneration, could reintroduce spec 87's own Goal-named
//! misclassification (`src/eventstore/contract.rs`, `src/blast_radius_eval.rs` counted as
//! production) even while the two resolvers still agree with each other in isolation. The four
//! tests after the byte-for-byte round-trip proof below close that gap and pin round 0's three
//! concrete fixed regressions (`adj-u87c2-r0-verdict-reject`) against the real committed file,
//! not just the implementer's synthetic fixtures or a reviewer's throwaway manual grep.

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
    /// Round 1 addition (`op-u87c2-round-1-ambiguity-covers-free-fns-too`): `file:line` of every
    /// OTHER production fn this bare name is shared with, populated exactly when `ambiguous` is
    /// `true`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    ambiguous_with: Vec<String>,
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
             (name/file/line/visibility/ambiguous/ambiguous_with/test_only_references, \
             test_only_references as file/line): {e}"
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

/// Round 1: `ambiguous_with` is populated EXACTLY when `ambiguous` is `true` (never the reverse,
/// never both empty-and-true or non-empty-and-false), and every citation it carries is a
/// non-blank `file:line`-shaped string a consumer can act on (e.g. to render "ambiguous with
/// `src/playbooks.rs:1`" in section 4).
#[test]
fn ambiguous_with_is_populated_iff_ambiguous_and_every_citation_is_file_colon_line_shaped() {
    let candidates = deserialize_committed_dead_code();
    for c in &candidates {
        assert_eq!(
            c.ambiguous,
            !c.ambiguous_with.is_empty(),
            "{c:?} has ambiguous/ambiguous_with out of sync"
        );
        for citation in &c.ambiguous_with {
            let Some((file, line)) = citation.rsplit_once(':') else {
                panic!(
                    "{} has a non-file:line ambiguous_with citation {citation:?}",
                    c.name
                );
            };
            assert!(
                !file.is_empty(),
                "{} has an ambiguous_with citation with an empty file: {citation:?}",
                c.name
            );
            assert!(
                line.parse::<usize>().is_ok_and(|n| n >= 1),
                "{} has an ambiguous_with citation with a non-positive line: {citation:?}",
                c.name
            );
        }
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

// -----------------------------------------------------------------------------------------
// ROUND 1: pinning the reference-class fixes against the REAL committed file, from outside.
// See the module doc comment's "ROUND 1 ACCOUNTING" section for why these four exist as a
// periphery layer distinct from the implementer's own fixture-driven unit tests.
// -----------------------------------------------------------------------------------------

/// The real tree's own out-of-line test files, named explicitly rather than re-derived through
/// the production pipeline's public API: spec 87's own Goal text names exactly these two
/// (`src/eventstore/contract.rs`, `src/blast_radius_eval.rs`) as the worked misclassification
/// example, and a fresh grep of the real tree today (`grep -rn 'cfg(test)' -A1 src/ | grep 'mod
/// [a-z_0-9]*;'`) finds no third: `src/eventstore/mod.rs` declares `#[cfg(test)] pub mod
/// contract;`, `src/lib.rs` declares `#[cfg(test)] mod blast_radius_eval;`. A hard-coded list
/// deliberately does NOT re-derive the production resolver's effect here (that would duplicate
/// `production_out_of_line_exclusion_set` in `tests/simplification_audit.rs`, which is already
/// exercised, on fixtures and the real tree, by the implementer's own `resolvers_agree_on_*`
/// tests) - this test's whole point is independence from that derivation: even if a future edit
/// broke the resolver-agreement property in a way neither resolver's own self-comparison could
/// see, a candidate from either of these two named files landing in the committed artifact would
/// still be caught here.
const KNOWN_OUT_OF_LINE_TEST_FILES: [&str; 2] =
    ["src/eventstore/contract.rs", "src/blast_radius_eval.rs"];

/// Round 1 class 3 (`op-u87c2-round-1-closes-the-reference-classes-not-the-instances`): the
/// exact misclassification spec 87's own Goal names by name must never reappear in the committed
/// artifact - checked here against the PERSISTED file, independent of whether the two resolvers
/// happen to still agree with each other on some future fixture.
#[test]
fn no_committed_candidate_comes_from_a_known_out_of_line_test_file() {
    let candidates = deserialize_committed_dead_code();
    for c in &candidates {
        assert!(
            !KNOWN_OUT_OF_LINE_TEST_FILES.contains(&c.file.as_str()),
            "{} ({}:{}) is listed as a production dead-code candidate, but {} is an out-of-line \
             test file (declared behind #[cfg(test)] elsewhere) - the exact misclassification \
             spec 87's own Goal names",
            c.name,
            c.file,
            c.line,
            c.file
        );
    }
}

/// Round 1 class 1 (mod-span test regions): `index_events` is spec 87's own Goal-cited worked
/// example (a fn referenced only from a `use` sitting at a `#[cfg(test)] mod tests { .. }` top
/// level, outside every fn body) - round 0 shipped a committed file where it was silently
/// absent (`sdet-u87c2-mod-body-level-test-statements-leak-as-production-refs`). Checked by name
/// and file only (not line): the fix this test guards is about mod-span test-region tracking,
/// not about `index_events`'s own definition site, so asserting its line would make this test
/// fail on any unrelated future edit that merely moves the function within its file.
#[test]
fn index_events_the_spec_goals_own_worked_example_is_present() {
    let candidates = deserialize_committed_dead_code();
    assert!(
        candidates
            .iter()
            .any(|c| c.name == "index_events" && c.file == "src/grounder/symbols/events.rs"),
        "index_events (src/grounder/symbols/events.rs) is absent from {DEAD_CODE_PATH} - a \
         regression of the mod-span test-region fix, spec 87's own Goal-cited worked example"
    );
}

/// Round 1 class 2 (attribute token trees are references): `default_build_config` is genuinely
/// live via `#[serde(default = "default_build_config")]` in `src/config.rs` - round 0 shipped a
/// false positive (`sdet-u87c2-serde-default-attr-string-ref-is-a-false-positive`) that would
/// have scheduled a real, live function for deletion in the wave.
#[test]
fn default_build_config_referenced_only_via_a_serde_default_attribute_is_absent() {
    let candidates = deserialize_committed_dead_code();
    assert!(
        !candidates.iter().any(|c| c.name == "default_build_config"),
        "default_build_config appears in {DEAD_CODE_PATH} - a regression of the \
         attribute-token-tree-reference fix: it is genuinely live via \
         #[serde(default = \"default_build_config\")] in src/config.rs"
    );
}

/// Round 1 class 4 (ambiguity is one class for every fn kind, adversary-found): two production
/// free fns named `rebuild` (`src/distiller.rs` and `src/playbooks.rs`) shared one bare-name
/// bucket; only `distiller::rebuild` has zero attributable references and must surface as
/// `ambiguous: true` naming its live namesake, rather than silently vanishing from the JSON
/// (`adv-u87c2-r0-free-fn-bare-name-collision-hides-a-genuinely-dead-fn`) - checked by the
/// `ambiguous_with` citation's FILE component only (not its line), so an unrelated future edit
/// that merely moves `rebuild` within `src/playbooks.rs` does not spuriously fail this test.
#[test]
fn distiller_rebuild_is_flagged_ambiguous_and_names_its_live_namesake_in_playbooks() {
    let candidates = deserialize_committed_dead_code();
    let rebuild = candidates
        .iter()
        .find(|c| c.name == "rebuild" && c.file == "src/distiller.rs")
        .unwrap_or_else(|| {
            panic!("rebuild (src/distiller.rs) is absent from {DEAD_CODE_PATH} entirely")
        });
    assert!(
        rebuild.ambiguous,
        "rebuild (src/distiller.rs:{}) is not flagged ambiguous, but a same-named live free fn \
         exists at src/playbooks.rs - a regression of the free-fn ambiguity fix",
        rebuild.line
    );
    assert!(
        rebuild.ambiguous_with.iter().any(|c| c
            .rsplit_once(':')
            .is_some_and(|(file, _)| file == "src/playbooks.rs")),
        "rebuild (src/distiller.rs:{})'s ambiguous_with {:?} does not cite src/playbooks.rs - a \
         consumer reading this entry cannot find the live namesake that keeps it ambiguous \
         rather than a confirmed deletion",
        rebuild.line,
        rebuild.ambiguous_with
    );
}
