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
//! which is also why the round-trip test below can assert byte-identical re-encoding.
//! `ambiguous_with` (round 1, `op-u87c2-round-1-ambiguity-covers-free-fns-too`) carries the SAME
//! `#[serde(default, skip_serializing_if = "Vec::is_empty")]` shape the producer declares it
//! with, so a non-ambiguous entry (the overwhelming majority) round-trips with no key for it at
//! all - proven by the same byte-identical comparison, not asserted separately.
//!
//! This unit does NOT own dispositions, the knowledge-graph degree cross-check, or report
//! section 4 (spec 87 Done-when: "criterion 3, NOT this one's"), so this file drives no binary
//! and spawns no process - the whole surface to prove is the persisted data contract itself.
//!
//! CRITERION 3 ACCOUNTING (u87c3, extending this file rather than starting a parallel one - the
//! same shared-artifact/shared-contract-test authority round 0-3 of criterion 2 already
//! established): criterion 3 adds `disposition` and `reason` to the SAME committed
//! `docs/audit/dead-code.json` (decision `u87c2-json-schema-excludes-disposition`: "c3 extends
//! this same struct/JSON when it lands"), so `ConsumedDeadCodeCandidate` below grows the same two
//! fields, as plain `String` (a real downstream consumer need not replicate the producer's own
//! `Disposition` enum type to read its wire value - the exhaustiveness check belongs to a test
//! that reads the three literal strings, below). Criterion 3 ALSO fixed a real bug in the
//! shared instrument while researching dispositions (decision
//! `u87c3-self-colon-colon-qualifier-false-positive`): `src/dash.rs`'s `DashMarker::parse`,
//! ambiguous with `gate.rs`/`ledger.rs` (x2)/`failure.rs`'s own `parse`s, was a false-positive
//! dead candidate - referenced only via `Self::parse(...)` from its own `DashMarker::read`
//! (a real production call path, `main.rs:5731/7313/7629`), which the qualifier-attribution
//! logic never resolved. The candidate count drops from 27 to 26 as a result; a regression test
//! below pins its continued absence.
//!
//! CRITERION 3, SDET-LENS ACCOUNTING (decision `sdet-u87c3-surface-accounting`): boundary probes
//! against base `84739b4` found the two new fields plus the disposition split test above (all
//! pre-existing, folded into the implementer's own commit) but one gap: spec 87 DISPOSITIONS is
//! stronger than "non-empty reason" - `keep-pending` "must cite the spec that will call it" and
//! `keep-public-surface` "must cite the consumer... a consumer that does not exist is not a
//! reason" - and none of the three pre-existing tests mechanically check citation SPECIFICITY
//! against the persisted file, only non-emptiness. The property held today (each of the 3 real
//! `keep-pending` entries already literally cites `spec 27`/`spec 32`/`spec 60`) but was unpinned.
//! `every_keep_pending_reason_cites_a_real_spec_number` below closes this at the `keep-pending`
//! side (a plain byte scan for `"spec "` immediately followed by an ASCII digit, the same
//! no-new-dependency style as every helper in this file). `keep-public-surface` has 0 real
//! entries today - vacuously satisfied, already pinned by the disposition-split test - so there
//! is no committed fact yet to assert its citation requirement against; a future entry needs its
//! own test when one lands. RED/GREEN discipline: backed up `docs/audit/dead-code.json`, stripped
//! every `"spec 27"` occurrence from the `distiller::rebuild` `keep-pending` reason in the
//! working copy, confirmed the new test fails naming that exact candidate and quoting the
//! corrupted reason, restored the original committed bytes (sha256-verified byte-identical), and
//! confirmed all 18 tests in this file green again.
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
//!
//! ROUND 2 ACCOUNTING (decision `sdet-u87c2-r2-surface-accounting`, superseding
//! `sdet-u87c2-r1-surface-accounting` above): round 1's own remedy was itself rejected
//! (`adj-u87c2-r1-verdict-reject`, upholding `sdet-u87c2-r1-impl-assoc-qualifier-drops-leading-
//! impl-generics-reintroduces-false-positives`) for reintroducing 4 real false positives -
//! `impl_assoc_qualifier`'s naive `split('<' | whitespace)` returned an EMPTY qualifier for any
//! impl header carrying its OWN leading generic/lifetime parameters. Round 2
//! (`u87c2-round-2-reuse-impl-self-type`) fixes this by delegating to the existing, separately
//! tested `impl_self_type` helper instead of reproving the same grammar; probes 1-4 all stay
//! empty (no new `pub` API, trait impl, CLI surface, or serialized-form field - the fix and its
//! one disclosed extension, a leading `dyn`/`impl` keyword strip, are both purely-internal
//! private-helper changes with no new cross-module seam). The one periphery-visible surface item:
//! the fix removes 4 named false-positive candidates from the committed artifact -
//! `Namespaced::new` (`src/eventstore/namespace.rs`), `ReplayDriver::new`
//! (`src/driver/replay.rs`), `Buckets::new` (`src/dash.rs`), `Server::new` (`src/mcpserver.rs`) -
//! a regression class round 1's periphery layer could not yet pin since it postdates round 1. The
//! ROUND 2 test after the round-1 tests below closes that gap. EXEMPT: the `dyn`/`impl`-keyword
//! strip itself has no committed-artifact fact to assert against (zero `impl dyn` blocks exist in
//! `src/` today, confirmed inert per `adv-u87c2-r1-cheaper-fix-exists-reuse-impl-self-type`); it
//! is correctly exercised only by the implementer's own unit test against synthetic input.
//!
//! ROUND 3 ACCOUNTING (decision `sdet-u87c2-r3-surface-accounting`, superseding
//! `sdet-u87c2-r2-surface-accounting` above): round 2's own remedy was itself rejected
//! (`adj-u87c2-r2-verdict-reject`, upholding two NEW false-positive classes -
//! `sdet-u87c2-r2-fnptr-struct-field-value-is-an-invisible-reference-shape` and
//! `sdet-u87c2-r2-method-category-relevant-filter-discards-ufcs-qualified-call-sites`). Round 3
//! (`op-u87c2-round-3-a-reference-is-any-token-not-a-shape`) removes the notion of reference
//! SHAPE entirely - `ref_shapes()` and `RefSite`'s `method_shaped`/`free_shaped` fields are
//! DELETED, not extended, and every non-definition `Ident` token equal to a candidate's name is
//! unconditionally a reference. Probes 1-4 all stay empty (`git diff 08fb056..b7826a8 -- '*.rs'`
//! adds no `pub` item, no trait impl, touches no `src/main.rs`, and adds no `derive`/`TYPE_` -
//! the diff *removes* two struct fields, it adds none); probe 5 (cross-module seam) is also
//! empty, read by hand - the whole diff is an internal rewrite of this one generator file, no new
//! call into `src/`, no new process spawn. The one periphery-visible surface item, larger than
//! either prior round's: THE RULE removes 16 named candidates (0 added) from the committed
//! artifact, in two distinct mechanisms.
//!
//! Mechanism A - a struct-literal field VALUE or a UFCS path used as a value on a
//! `DispatchCategory::Method` fn, exactly the two round-2 upheld classes, now genuinely fixed for
//! their reported instances AND for further real instances neither round 2 nor the operator's
//! ruling named: the 10 `src/docs.rs` `skill_registry()` `render_*` fns (struct-literal field
//! value, e.g. `render_body: render_planning_a_spec_skill,` at `src/docs.rs:1211`) and
//! `src/config.rs`'s `to_rule` (UFCS value, `.map(FailureRuleDef::to_rule)` at
//! `src/config.rs:766`) are the 11 the operator's ruling explicitly named. `is_grep_fallback`
//! (`src/progress.rs`, UFCS value `.filter(crate::progress::AgentProgress::is_grep_fallback)` at
//! `src/metrics.rs:1066`) and `is_snapshot_drift` (`src/metrics.rs`, UFCS value
//! `.all(ModelChange::is_snapshot_drift)` at `src/metrics.rs:1333`) are two MORE real,
//! previously-unreported instances of the identical Method-category UFCS-value class - genuine
//! evidence the round-3 fix closes the CLASS, not merely the two reported occurrences.
//!
//! Mechanism B - the explicit, accepted precision trade THE RULE's own text states ("a local
//! variable or struct field sharing a fn's bare name now keeps that fn looking alive too - a
//! false negative, never a false positive"): `placements` (`src/eventstore/mod.rs`, kept alive by
//! its own struct's same-named field, e.g. `self.placements` at `src/eventstore/mod.rs:173`),
//! `written` (`src/watch.rs`, kept alive by the `written` binding in the `matches!` pattern at
//! `src/watch.rs:528`), and `rules` (`src/failure.rs`, kept alive by `Taxonomy`'s own `rules`
//! field, e.g. `self.rules.iter()` at `src/failure.rs:238`) each verified by hand to have NO
//! genuine call-shaped production reference of their own - each is provably dead by spec 87's own
//! definition, kept off this round's dead-code list only by the accepted trade.
//!
//! TESTED: three new periphery tests below the ROUND 2 test, each pinning one distinct claim
//! against the REAL committed file (never the generator's in-memory state, same independence
//! discipline as every round before it) - the 11 operator-ruling-named entries; the 2 further
//! real Method-category-UFCS instances (mechanism A's generality); the 3 field/local-collision
//! instances (mechanism B's precision trade, made visible in the persisted artifact rather than
//! resting on the fix's own prose).
//!
//! SPEC 90 CRITERION 2 ACCOUNTING (decision `sdet-u90c2-surface-accounting`): CLAIM 1 ("the
//! guarded catalog carries no line numbers") needs no new test - the pre-existing
//! `deserializing_then_reserializing_the_committed_dead_code_json_reproduces_the_committed_
//! bytes_exactly` below already proves it strictly (any stray key, `"line"` included, breaks
//! byte-exact re-encoding through a struct that lacks it). CLAIM 4 ("the report still cites
//! file:line from the unguarded lines file") had NO test anywhere before this unit, unit-level
//! or periphery - `the_committed_report_cites_dead_code_file_line_exactly_as_the_lines_sibling_
//! records_them` below closes it for section 4.3, reading the PERSISTED report and the PERSISTED
//! `DEAD_CODE_LINES_PATH` directly, joined by array position (never a `(file, name)` lookup -
//! `src/ingest.rs`'s own `ingest_project` is ambiguous, two distinct candidates sharing one bare
//! name in one file, so a lookup would silently resolve every citation to whichever entry comes
//! first). ROUND 4 (`adj-u90c2-r3-verdict-reject` named section 6 item 0, the dead-code deletion
//! list, alongside sections 1/4.3 as needing the identical sidecar-sourcing fix - the producer
//! delivered it, `render_dead_code_deletion_list` now takes an explicit `lines:
//! &[DeadCodeCandidateLines]` param, but no periphery test closed CLAIM 4 for that site):
//! `the_committed_report_section_6_deletion_list_cites_dead_code_file_line_exactly_as_the_lines_
//! sibling_records_them` below closes it the same way, filtered to `disposition == "delete"` (the
//! deletion list's own scope) and joined by the SAME array position as section 4.3's test, not
//! re-sorted. CLAIM 2 (pin-bump byte-identical) and CLAIM 3 (merge-friendly) both require
//! regenerating over a synthetic fixture tree via `build_dead_code_candidates`/`scan_tree`,
//! private to `tests/simplification_audit.rs`'s own `mod tests` - this layer never authors or
//! edits the unit's inside-out unit tests. Both claims are proven there for dead-code.json too as
//! of round 2 (`u90c2-r2-complete`; independently reconfirmed by the round-3 adjudicator,
//! `sdet-u90c2-r3-deadcode-map-claim-gap-confirmed-closed` inside `adj-u90c2-r3-verdict-reject` -
//! CLAIM markers 1/2/3 at tests/simplification_audit.rs:10428/10482/10520 on the current, round-4
//! tree; the adjudicator's own citation was against the pre-round-4 line count) - EXEMPT here, not
//! a gap: still out of periphery reach by the same white-box-only reasoning, now genuinely closed
//! rather than merely claimed.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Mirrors `tests/simplification_audit.rs`'s private `TestOnlyRefWire` shape field-for-field,
/// from the outside - see the module doc comment for why this is a deliberate re-declaration,
/// not an import. Spec 90 criterion 2: the guarded file carries `content_hash`, never `line` -
/// `line` moved to the unguarded `docs/audit/dead-code.lines.json` sibling, out of scope for
/// this file (never drift-guarded, so not a "documented contract" a downstream reader pins
/// against).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
struct ConsumedTestOnlyRef {
    file: String,
    content_hash: String,
}

/// Mirrors `tests/simplification_audit.rs`'s private `DeadCodeCandidateWire` shape
/// field-for-field, `disposition`/`reason` (criterion 3's own addition) included, as plain
/// `String` - see the module doc comment's CRITERION 3 ACCOUNTING for why a raw string, not the
/// producer's enum. Spec 90 criterion 2: `line` moved to the unguarded `.lines.json` sibling,
/// replaced by `content_hash`; `ambiguous_with` citations are `file#hash`, never `file:line`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
struct ConsumedDeadCodeCandidate {
    name: String,
    file: String,
    content_hash: String,
    visibility: String,
    ambiguous: bool,
    /// Round 1 addition (`op-u87c2-round-1-ambiguity-covers-free-fns-too`): `file#hash` (spec 90
    /// criterion 2 - was `file:line`) of every OTHER production fn this bare name is shared
    /// with, populated exactly when `ambiguous` is `true`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    ambiguous_with: Vec<String>,
    test_only_references: Vec<ConsumedTestOnlyRef>,
    disposition: String,
    reason: String,
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
             (name/file/content_hash/visibility/ambiguous/ambiguous_with/test_only_references/ \
             disposition/reason, test_only_references as file/content_hash): {e}"
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
/// references" (spec 90 criterion 2: the line half of that identity moved to the unguarded
/// `.lines.json` sibling - `content_hash` is this guarded file's line-free replacement). A
/// consumer reading an entry to render section 4 or schedule a deletion must never see a blank
/// name, a file outside `src/` (this criterion's whole point is PRODUCTION fns only - checked
/// here against the PERSISTED file, not `real_dead_code_candidates()`'s in-memory value the way
/// the unit's own
/// `every_real_candidate_has_a_zero_degree_knowledge_graph_cross_check_shape` does), an empty
/// content_hash, or an unrecognized visibility qualifier.
#[test]
fn every_deserialized_candidate_has_a_non_empty_name_a_src_file_a_content_hash_and_a_recognized_visibility(
) {
    let candidates = deserialize_committed_dead_code();
    for c in &candidates {
        assert!(!c.name.is_empty(), "{c:?} has an empty name");
        assert!(
            c.file.starts_with("src/"),
            "{c:?} has a file outside src/ - this criterion's whole point is production fns only"
        );
        assert!(
            !c.content_hash.is_empty(),
            "{c:?} has an empty content_hash"
        );
        assert!(
            c.visibility == "private" || c.visibility.starts_with("pub"),
            "{c:?} has an unrecognized visibility {:?}",
            c.visibility
        );
    }
}

/// Round 1: `ambiguous_with` is populated EXACTLY when `ambiguous` is `true` (never the reverse,
/// never both empty-and-true or non-empty-and-false), and every citation it carries is a
/// non-blank `file#content_hash`-shaped string a consumer can act on (spec 90 criterion 2:
/// `file:line` moved to the unguarded `.lines.json` sibling - this guarded file's citations are
/// line-free).
#[test]
fn ambiguous_with_is_populated_iff_ambiguous_and_every_citation_is_file_hash_shaped() {
    let candidates = deserialize_committed_dead_code();
    for c in &candidates {
        assert_eq!(
            c.ambiguous,
            !c.ambiguous_with.is_empty(),
            "{c:?} has ambiguous/ambiguous_with out of sync"
        );
        for citation in &c.ambiguous_with {
            let Some((file, hash)) = citation.rsplit_once('#') else {
                panic!(
                    "{} has a non-file#hash ambiguous_with citation {citation:?}",
                    c.name
                );
            };
            assert!(
                !file.is_empty(),
                "{} has an ambiguous_with citation with an empty file: {citation:?}",
                c.name
            );
            assert!(
                !hash.is_empty(),
                "{} has an ambiguous_with citation with an empty content_hash: {citation:?}",
                c.name
            );
        }
    }
}

/// Every test-only reference is an identity a reader can act on: a non-empty file and a
/// non-empty content_hash (spec 90 criterion 2: `line` moved to the unguarded `.lines.json`
/// sibling). A consumer must never be handed a blank-named or blank-hash reference. Spec 87
/// Constraints Walk permits an entry with ZERO test-only references (a fn referenced nowhere at
/// all, not even from a test) so this deliberately asserts nothing about count, only
/// per-reference shape.
#[test]
fn every_deserialized_test_only_reference_has_a_non_empty_file_and_content_hash() {
    let candidates = deserialize_committed_dead_code();
    for c in &candidates {
        for r in &c.test_only_references {
            assert!(
                !r.file.is_empty(),
                "{} has a test_only_reference with an empty file",
                c.name
            );
            assert!(
                !r.content_hash.is_empty(),
                "{} has a test_only_reference in {} with an empty content_hash",
                c.name,
                r.file
            );
        }
    }
}

/// Spec 90 criterion 2: the UNGUARDED sibling carrying [`DEAD_CODE_PATH`]'s line data, joined to
/// it by array POSITION (never drift-guarded, so not itself a "documented contract" - read here
/// only to prove the join and the ordering it preserves).
const DEAD_CODE_LINES_PATH: &str = "docs/audit/dead-code.lines.json";

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct ConsumedTestOnlyRefLines {
    file: String,
    line: usize,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct ConsumedDeadCodeCandidateLines {
    file: String,
    name: String,
    line: usize,
    test_only_references: Vec<ConsumedTestOnlyRefLines>,
}

fn deserialize_committed_dead_code_lines() -> Vec<ConsumedDeadCodeCandidateLines> {
    let path = repo_root().join(DEAD_CODE_LINES_PATH);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{DEAD_CODE_LINES_PATH} is missing or unreadable ({e})"));
    serde_json::from_str(&raw).unwrap_or_else(|e| {
        panic!("{DEAD_CODE_LINES_PATH} does not deserialize as the documented lines contract: {e}")
    })
}

/// Determinism/ordering, checked against the PERSISTED files rather than the generator's
/// in-memory value (the producer's own `build_dead_code_candidates` sorts its output by
/// `(file, line)` three times over - `tests/simplification_audit.rs`'s internal fixture tests
/// only ever assert individual entries' presence or absence, never that the committed ordering
/// itself is stable). Spec 90 criterion 2 moved the line data itself to the unguarded
/// `.lines.json` sibling, joined to the guarded file by array POSITION - so this now proves TWO
/// things: the join holds (same `(file, name)` at every index across both files) and the
/// ordering guarantee a consumer iterating `DEAD_CODE_PATH` for per-file grouping (e.g. section
/// 4's distribution table) depends on survives the split, relocated to the sidecar.
#[test]
fn the_committed_dead_code_json_and_its_lines_sibling_are_position_joined_and_ascending_by_file_then_line(
) {
    let candidates = deserialize_committed_dead_code();
    let lines = deserialize_committed_dead_code_lines();
    assert_eq!(
        candidates.len(),
        lines.len(),
        "{DEAD_CODE_PATH} and {DEAD_CODE_LINES_PATH} must have equal length - they are joined \
         by array position"
    );
    for (i, (c, l)) in candidates.iter().zip(lines.iter()).enumerate() {
        assert_eq!(
            (&c.file, &c.name),
            (&l.file, &l.name),
            "{DEAD_CODE_PATH} and {DEAD_CODE_LINES_PATH} entry {i} disagree on (file, name) - \
             the two files are joined by array position and must describe the same candidate at \
             each index"
        );
        assert_eq!(
            c.test_only_references.len(),
            l.test_only_references.len(),
            "{}'s test_only_references length disagrees between {DEAD_CODE_PATH} and \
             {DEAD_CODE_LINES_PATH}",
            c.name
        );
    }
    let mut sorted = lines.clone();
    sorted.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    assert_eq!(
        lines, sorted,
        "{DEAD_CODE_LINES_PATH} is not already ascending by (file, line)"
    );
    for l in &lines {
        let mut refs_sorted = l.test_only_references.clone();
        refs_sorted.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
        assert_eq!(
            l.test_only_references, refs_sorted,
            "{}'s test_only_references in {DEAD_CODE_LINES_PATH} are out of (file, line) order",
            l.name
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
/// mechanical proof that no field beyond the eight declared above is present in the file today:
/// an extra field would silently drop on decode and then fail this exact comparison.
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
            "{} ({}) is listed as a production dead-code candidate, but {} is an out-of-line \
             test file (declared behind #[cfg(test)] elsewhere) - the exact misclassification \
             spec 87's own Goal names",
            c.name,
            c.file,
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
/// `ambiguous_with` citation's FILE component only (not its content_hash, spec 90 criterion 2's
/// line-free replacement for the citation's old line component), so an unrelated future edit
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
        "rebuild (src/distiller.rs) is not flagged ambiguous, but a same-named live free fn \
         exists at src/playbooks.rs - a regression of the free-fn ambiguity fix"
    );
    assert!(
        rebuild.ambiguous_with.iter().any(|c| c
            .rsplit_once('#')
            .is_some_and(|(file, _)| file == "src/playbooks.rs")),
        "rebuild (src/distiller.rs)'s ambiguous_with {:?} does not cite src/playbooks.rs - a \
         consumer reading this entry cannot find the live namesake that keeps it ambiguous \
         rather than a confirmed deletion",
        rebuild.ambiguous_with
    );
}

// -----------------------------------------------------------------------------------------
// ROUND 2: pinning the round-2 fix against the REAL committed file, from outside. See the
// module doc comment's "ROUND 2 ACCOUNTING" section for the boundary-probe rerun this test
// closes.
// -----------------------------------------------------------------------------------------

/// Round 2 (`u87c2-round-2-reuse-impl-self-type`, closing round 1's upheld defect
/// `sdet-u87c2-r1-impl-assoc-qualifier-drops-leading-impl-generics-reintroduces-false-positives`,
/// `adj-u87c2-r1-verdict-reject`): four live, widely-used constructors, each declared inside an
/// `impl` block that carries ITS OWN leading generic/lifetime parameters
/// (`impl<'a> Namespaced<'a>`, `impl<'a> ReplayDriver<'a>`, `impl<'g> Buckets<'g>`,
/// `impl<'a> Server<'a>`), were false-flagged as zero-production-reference dead-code candidates
/// in round 1's committed artifact: `impl_assoc_qualifier`'s naive
/// `header.split(|c| c == '<' || c.is_whitespace()).next()` returned an EMPTY qualifier for a
/// header that starts with `<` itself (the `impl` keyword is never stored in `enclosing_impl`),
/// so it could never match the real `Type::name(`-shaped call sites that keep these constructors
/// genuinely alive - the exact false-positive-feeds-a-possible-deletion direction spec 87's
/// design says must never happen. Checked here by name+file only (not line): the fix is about
/// qualifier RESOLUTION, not about any of these four functions' own definition sites, so an
/// unrelated future edit that merely moves one within its file must not spuriously fail this
/// test.
#[test]
fn generic_impl_header_constructors_previously_false_flagged_are_absent_from_the_committed_file() {
    let candidates = deserialize_committed_dead_code();
    for (name, file) in [
        ("new", "src/eventstore/namespace.rs"), // Namespaced::new
        ("new", "src/driver/replay.rs"),        // ReplayDriver::new
        ("new", "src/dash.rs"),                 // Buckets::new
        ("new", "src/mcpserver.rs"),            // Server::new
    ] {
        assert!(
            !candidates.iter().any(|c| c.name == name && c.file == file),
            "{name} ({file}) appears in {DEAD_CODE_PATH} - a regression of the round-2 \
             impl_assoc_qualifier-reuses-impl_self_type fix: this constructor's enclosing impl \
             block declares its own leading generic/lifetime parameters, and the pre-fix naive \
             qualifier split returned empty for that header shape, silently dropping its real \
             qualified call sites and false-flagging it dead"
        );
    }
}

// -----------------------------------------------------------------------------------------
// ROUND 3: pinning the round-3 "any token, not a shape" fix against the REAL committed file,
// from outside. See the module doc comment's "ROUND 3 ACCOUNTING" section for the boundary-probe
// rerun and the two mechanisms these three tests each close.
// -----------------------------------------------------------------------------------------

/// Round 3 (`op-u87c2-round-3-a-reference-is-any-token-not-a-shape`), the 11 entries the
/// operator's ruling explicitly named after `adj-u87c2-r2-verdict-reject` upheld
/// `sdet-u87c2-r2-fnptr-struct-field-value-is-an-invisible-reference-shape` (a struct-literal
/// field VALUE has no reference shape at all) and
/// `sdet-u87c2-r2-method-category-relevant-filter-discards-ufcs-qualified-call-sites` (a UFCS
/// value on a `Method`-category fn was discarded by `relevant()`'s shape gate even though
/// `ref_shapes()` already saw it). Checked by name+file only (not line): the fix is about
/// reference RECOGNITION, not about any of these functions' own definition sites, so an
/// unrelated future edit that merely moves one within its file must not spuriously fail this
/// test.
#[test]
fn value_position_and_ufcs_reference_shapes_previously_invisible_are_absent_from_the_committed_file(
) {
    let candidates = deserialize_committed_dead_code();
    for (name, file) in [
        // src/docs.rs skill_registry()'s 10 render_body: render_*_skill struct-literal field
        // values (src/docs.rs:1207-1243) - the fnptr-struct-field-value class.
        ("render_using_rigger_skill", "src/docs.rs"),
        ("render_planning_a_spec_skill", "src/docs.rs"),
        ("render_reset_store_skill", "src/docs.rs"),
        ("render_build_graph_skill", "src/docs.rs"),
        ("render_reindex_skill", "src/docs.rs"),
        ("render_resume_a_run_skill", "src/docs.rs"),
        ("render_handle_an_escalation_skill", "src/docs.rs"),
        ("render_watch_a_run_skill", "src/docs.rs"),
        ("render_restore_the_dash_skill", "src/docs.rs"),
        ("render_diagnose_churn_skill", "src/docs.rs"),
        // src/config.rs's .map(FailureRuleDef::to_rule) at src/config.rs:766 - the
        // Method-category-UFCS-value class.
        ("to_rule", "src/config.rs"),
    ] {
        assert!(
            !candidates.iter().any(|c| c.name == name && c.file == file),
            "{name} ({file}) appears in {DEAD_CODE_PATH} - a regression of the round-3 \
             any-token-not-a-shape fix: this fn is genuinely referenced as a value (a \
             struct-literal field value or a UFCS path), a shape no prior round's scanner \
             recognized as a reference at all"
        );
    }
}

/// Round 3, mechanism A's GENERALITY: `is_grep_fallback` and `is_snapshot_drift` are two MORE
/// real, previously-UNREPORTED instances of the exact same `DispatchCategory::Method`
/// UFCS-value-to-a-combinator class `to_rule` was the one reported instance of
/// (`sdet-u87c2-r2-method-category-relevant-filter-discards-ufcs-qualified-call-sites`) -
/// `src/metrics.rs:1066`'s `.filter(crate::progress::AgentProgress::is_grep_fallback)` and
/// `src/metrics.rs:1333`'s `.all(ModelChange::is_snapshot_drift)`, verified by hand against the
/// real tree, neither cited in the operator's round-3 ruling or the round-2 upheld findings.
/// Their absence here is independent proof the round-3 fix closes the CLASS ("no code decides
/// whether an occurrence looks like a call" - `op-u87c2-round-3-a-reference-is-any-token-not-a-
/// shape`), not just the one instance every prior round's periphery layer could name.
#[test]
fn the_general_ufcs_method_value_fix_also_closes_previously_unreported_same_class_instances() {
    let candidates = deserialize_committed_dead_code();
    for (name, file) in [
        ("is_grep_fallback", "src/progress.rs"),
        ("is_snapshot_drift", "src/metrics.rs"),
    ] {
        assert!(
            !candidates.iter().any(|c| c.name == name && c.file == file),
            "{name} ({file}) appears in {DEAD_CODE_PATH} - this is a real, previously-unreported \
             instance of the same Method-category UFCS-value class the round-3 fix was supposed \
             to close generally, not merely the one reported instance (to_rule)"
        );
    }
}

/// Round 3, mechanism B - THE RULE's own explicitly accepted precision trade ("a local variable
/// or struct field sharing a fn's bare name now keeps that fn looking alive too - a false
/// negative, never a false positive"): `placements` (kept alive by `Appended`'s own
/// `self.placements` field access, e.g. `src/eventstore/mod.rs:173`), `written` (kept alive by
/// the `written` binding in a `matches!` pattern at `src/watch.rs:528`), and `rules` (kept alive
/// by `Taxonomy`'s own `self.rules` field access, e.g. `src/failure.rs:238`) each have NO
/// call-shaped production reference of their own - verified by hand, each is provably dead by
/// spec 87's own definition, kept off the committed list only by the accepted trade. This test
/// exists so the trade stays VISIBLE in the persisted artifact rather than resting only on the
/// fix's own prose: a future edit that renamed the colliding field/local without genuinely
/// reviving the method would silently reintroduce these as real dead-code candidates, and this
/// test would start failing exactly then - a signal, not a bug, but one worth naming rather than
/// leaving mute.
#[test]
fn getter_methods_kept_alive_only_by_a_same_named_production_field_or_local_are_also_absent() {
    let candidates = deserialize_committed_dead_code();
    for (name, file) in [
        ("placements", "src/eventstore/mod.rs"),
        ("written", "src/watch.rs"),
        ("rules", "src/failure.rs"),
    ] {
        assert!(
            !candidates.iter().any(|c| c.name == name && c.file == file),
            "{name} ({file}) appears in {DEAD_CODE_PATH} - the accepted same-named-field/local \
             false-negative trade (op-u87c2-round-3-a-reference-is-any-token-not-a-shape) no \
             longer holds for this entry; either the colliding token was removed (in which case \
             this fn may now be genuinely dead and belongs on the list with a real disposition) \
             or the rule regressed"
        );
    }
}

// -----------------------------------------------------------------------------------------
// CRITERION 3 (`u87c3`, THIS UNIT): dispositions land in the SAME committed artifact. See the
// module doc comment's "CRITERION 3 ACCOUNTING" section.
// -----------------------------------------------------------------------------------------

/// Spec 87 DISPOSITIONS: "exactly three" - `delete`, `keep-public-surface`, `keep-pending` -
/// and "Every entry gets one; an entry without a cited reason is a defect". Checked against the
/// PERSISTED file (never the producer's in-memory value), exactly the independence this whole
/// file exists to prove for every other field.
#[test]
fn every_committed_candidate_has_exactly_one_of_the_three_dispositions_with_a_non_empty_reason() {
    let candidates = deserialize_committed_dead_code();
    assert!(!candidates.is_empty(), "expected committed candidates");
    for c in &candidates {
        assert!(
            ["delete", "keep-public-surface", "keep-pending"].contains(&c.disposition.as_str()),
            "{} ({}) has an unrecognized disposition {:?} - spec 87 names exactly three",
            c.name,
            c.file,
            c.disposition
        );
        assert!(
            !c.reason.trim().is_empty(),
            "{} ({}) has an empty disposition reason",
            c.name,
            c.file
        );
    }
}

/// Regression pin for the real bug criterion 3 found and fixed while researching dispositions
/// (decision `u87c3-self-colon-colon-qualifier-false-positive`, see the module doc comment):
/// `DashMarker::parse` (`src/dash.rs:398`) is referenced only via `Self::parse(...)` from its own
/// `DashMarker::read`, itself called in real production code (`main.rs:5731/7313/7629`) - it must
/// never again appear as a dead-code candidate, which would recommend deleting live code.
#[test]
fn dash_marker_parse_the_self_colon_colon_false_positive_stays_absent() {
    let candidates = deserialize_committed_dead_code();
    assert!(
        !candidates
            .iter()
            .any(|c| c.name == "parse" && c.file == "src/dash.rs"),
        "src/dash.rs's parse (DashMarker::parse) appears in {DEAD_CODE_PATH} - a regression of \
         the Self:: qualifier-attribution fix (u87c3-self-colon-colon-qualifier-false-positive); \
         it is called from real production code via Self::parse inside DashMarker::read and must \
         never be recommended for deletion"
    );
}

/// The exact 23/3/0 `delete`/`keep-pending`/`keep-public-surface` split this criterion's research
/// established, pinned against the persisted file (mirrors
/// `the_real_tree_disposition_split_matches_this_criterions_research` in
/// `tests/simplification_audit.rs`, checked there against the in-memory producer value - this is
/// the same fact, independently re-derived from the committed bytes). Was 22/3/0 (25 total) until
/// spec 92 criterion 1's FRESH ON EVERY INTEGRATION unit added `record_current_generation`
/// (src/ingest.rs, a private test-only fixture helper, `delete`) to `src/ingest.rs`'s own
/// `scoped_reindex_tests` module, raising the total to 26 and `delete` to 23. Was 23/3/0 (26
/// total) until spec 88 criterion 1 round 4 moved `expect_merged` (src/worktree.rs, `delete`) out
/// of production entirely (into `src/worktree.rs`'s own `#[cfg(test)] mod tests`, alongside
/// `IntegrateOutcome` and a test-only `integrate` recomposition of the newly-split
/// `merge_into_worktree`/`land`), dropping the total to 25 and `delete` to 22 - and now back up
/// to 26/23 by the unrelated addition above.
#[test]
fn the_committed_dead_code_json_disposition_split_is_23_delete_3_keep_pending_0_keep_public_surface(
) {
    let candidates = deserialize_committed_dead_code();
    let delete = candidates
        .iter()
        .filter(|c| c.disposition == "delete")
        .count();
    let keep_public = candidates
        .iter()
        .filter(|c| c.disposition == "keep-public-surface")
        .count();
    let keep_pending = candidates
        .iter()
        .filter(|c| c.disposition == "keep-pending")
        .count();
    assert_eq!(
        (candidates.len(), delete, keep_public, keep_pending),
        (26, 23, 0, 3),
        "the committed disposition split has changed since this criterion's research"
    );
}

/// Plain byte scan for a `"spec <digits>"` citation - no new dependency, matching this file's
/// own no-regex-crate style. Deliberately stricter than a bare substring search for `"spec"`:
/// the word alone (as in "inspect" or ordinary English prose) does not satisfy spec 87
/// DISPOSITIONS' citation requirement, only `"spec"` immediately followed by a space and at
/// least one ASCII digit does.
fn cites_a_spec_number(reason: &str) -> bool {
    let mut rest = reason;
    while let Some(idx) = rest.find("spec ") {
        rest = &rest[idx + "spec ".len()..];
        if rest.as_bytes().first().is_some_and(u8::is_ascii_digit) {
            return true;
        }
    }
    false
}

/// Spec 87 DISPOSITIONS is stronger than "non-empty reason" (already pinned above by
/// `every_committed_candidate_has_exactly_one_of_the_three_dispositions_with_a_non_empty_reason`):
/// `keep-pending` "must cite the spec that will call it". Checked against the PERSISTED file's
/// actual `keep-pending` reasons - a reason that merely uses the ENGLISH WORD "spec" without a
/// number, or omits it entirely, is a defect this test catches that the non-emptiness check
/// cannot. (`keep-public-surface`'s parallel "must cite the consumer... a consumer that does not
/// exist is not a reason" clause has no committed candidate to check today - 0 real entries,
/// already pinned vacuously by the disposition-split test above - so there is nothing yet to
/// mechanically assert there.)
#[test]
fn every_keep_pending_reason_cites_a_real_spec_number() {
    let candidates = deserialize_committed_dead_code();
    let keep_pending: Vec<_> = candidates
        .iter()
        .filter(|c| c.disposition == "keep-pending")
        .collect();
    assert!(
        !keep_pending.is_empty(),
        "expected at least one keep-pending candidate in {DEAD_CODE_PATH}"
    );
    for c in &keep_pending {
        assert!(
            cites_a_spec_number(&c.reason),
            "{} ({}) has disposition keep-pending but its reason does not cite a \"spec N\" \
             number - spec 87 DISPOSITIONS requires keep-pending to \"cite the spec that will \
             call it\": {:?}",
            c.name,
            c.file,
            c.reason
        );
    }
}

// -----------------------------------------------------------------------------------------
// Spec 90 criterion 2, THE DRIFT GUARD IS LINE-FREE: CLAIM 4 ("the report still cites
// file:line from the unguarded lines file"), proven against the PERSISTED report and the
// PERSISTED lines sibling - never a regenerated in-memory value. CLAIM 1 ("the guarded
// catalog carries no line numbers") needs no separate test here: the pre-existing
// `deserializing_then_reserializing_the_committed_dead_code_json_reproduces_the_committed_
// bytes_exactly` below already proves it, strictly - `ConsumedDeadCodeCandidate`/
// `ConsumedTestOnlyRef` have no `#[serde(deny_unknown_fields)]`, so a stray `"line"` key would
// deserialize-and-silently-drop, but re-serializing without it would then produce BYTES
// SHORTER than the committed file by that key, failing byte-exact equality - a strictly
// stronger, already-covered check than a raw-key scan would add on its own.
// -----------------------------------------------------------------------------------------

const REPORT_PATH: &str = "docs/audit/2026-09-simplification-audit.md";

/// Section 4.3's per-candidate citation shape (`render_dead_code_full_list`'s own template:
/// `` - **{name}** (`{file}:{line}`, ... ``), extracted from the section between its own "### 4.3"
/// heading and the next top-level "## " heading so a `src/some.rs:N` appearing in some OTHER
/// section's free prose is never mistaken for one of this section's mechanically-rendered
/// citations.
fn section_4_3_citations(report: &str) -> Vec<(String, String, usize)> {
    let start = report
        .find("### 4.3 The full list, dispositioned")
        .expect("report has a 4.3 heading");
    let rest = &report[start..];
    let end = rest
        .find("\n## ")
        .map(|i| i + start)
        .unwrap_or(report.len());
    let section = &report[start..end];
    let re = regex::Regex::new(r"\*\*([^*]+)\*\* \(`([^`:]+):(\d+)`").expect("valid regex");
    re.captures_iter(section)
        .map(|c| {
            (
                c[1].to_string(),
                c[2].to_string(),
                c[3].parse().expect("digits"),
            )
        })
        .collect()
}

/// CLAIM 4: "the report still cites `file:line` from the unguarded lines file." Every
/// mechanically-rendered `(name, file, line)` citation in section 4.3 of the COMMITTED report
/// (never `render_dead_code_full_list`'s own in-memory rendering - nothing above or in the
/// implementer's own suite reads the persisted report file at all for this artifact) matches, in
/// ORDER, the PERSISTED `DEAD_CODE_LINES_PATH` - both are rendered from the SAME underlying
/// candidate sequence with no re-sorting (`render_dead_code_full_list` iterates
/// `real_dead_code_candidates()` untouched), so a position-wise zip is the correct join, not a
/// `(file, name)` lookup: `src/ingest.rs`'s own `ingest_project` is ambiguous (two distinct
/// candidates share one bare name in one file, ROUND 1's own `ambiguous_with` shape), so a lookup
/// would silently resolve every citation to whichever entry happens to come first.
#[test]
fn the_committed_report_cites_dead_code_file_line_exactly_as_the_lines_sibling_records_them() {
    let report = std::fs::read_to_string(repo_root().join(REPORT_PATH))
        .unwrap_or_else(|e| panic!("{REPORT_PATH} is missing or unreadable ({e})"));
    let citations = section_4_3_citations(&report);
    let lines = deserialize_committed_dead_code_lines();
    assert_eq!(
        citations.len(),
        lines.len(),
        "section 4.3 of {REPORT_PATH} cites {} candidates but {DEAD_CODE_LINES_PATH} records {} \
         - they must list the same candidates in the same order (both come from the same \
         underlying sequence, untouched)",
        citations.len(),
        lines.len()
    );
    assert!(
        !citations.is_empty(),
        "found zero section 4.3 citations in {REPORT_PATH} - the extraction regex or the \
         section boundary is broken"
    );
    for (i, ((name, file, line), entry)) in citations.iter().zip(lines.iter()).enumerate() {
        assert_eq!(
            (file.as_str(), name.as_str()),
            (entry.file.as_str(), entry.name.as_str()),
            "citation {i} in section 4.3 of {REPORT_PATH} is {name} ({file}), but \
             {DEAD_CODE_LINES_PATH} entry {i} is {} ({}) - report order and lines-sibling order \
             have diverged",
            entry.name,
            entry.file
        );
        assert_eq!(
            entry.line, *line,
            "report section 4.3 cites {name} ({file}:{line}), but {DEAD_CODE_LINES_PATH} records \
             line {} for that same candidate",
            entry.line
        );
    }
}

/// Section 6 item 0's own per-file deletion list (`render_dead_code_deletion_list`'s own
/// template: `` - `{file}`: `{name}` (line {N}), ... ``, one line per file, entries within a file
/// comma-joined on the SAME line), extracted from the section between its own
/// "#### 0. Delete the dead-code set" heading and the next "#### " heading so a `src/some.rs:N`
/// appearing in some OTHER plan item's free prose is never mistaken for one of this list's
/// mechanically-rendered citations. Each per-file line is captured whole first (`line_re`), then
/// its own comma-joined entries are pulled out in order (`entry_re`) - a single combined regex
/// cannot distinguish "which file does this entry belong to" once more than one file groups onto
/// the report, since entries never repeat their file inline.
fn section_6_deletion_list_citations(report: &str) -> Vec<(String, String, usize)> {
    let start = report
        .find("#### 0. Delete the dead-code set")
        .expect("report has a '#### 0. Delete the dead-code set' heading");
    let rest = &report[start..];
    let end = rest
        .find("\n#### ")
        .map(|i| i + start)
        .unwrap_or(report.len());
    let section = &report[start..end];
    let line_re = regex::Regex::new(r"(?m)^  - `([^`]+)`: (.+)$").expect("valid regex");
    let entry_re = regex::Regex::new(r"`([^`]+)` \(line (\d+)\)").expect("valid regex");
    let mut out = Vec::new();
    for line_cap in line_re.captures_iter(section) {
        let file = line_cap[1].to_string();
        for entry_cap in entry_re.captures_iter(&line_cap[2]) {
            out.push((
                file.clone(),
                entry_cap[1].to_string(),
                entry_cap[2].parse().expect("digits"),
            ));
        }
    }
    out
}

/// CLAIM 4, section 6 item 0 (round 4's own extent - `adj-u90c2-r3-verdict-reject` named this
/// site alongside sections 1/4.3): every mechanically-rendered `(name, file, line)` citation in
/// the COMMITTED report's dead-code deletion list matches, in order, the PERSISTED
/// `DEAD_CODE_LINES_PATH` filtered to `disposition == "delete"` in
/// [`deserialize_committed_dead_code`]. `render_dead_code_deletion_list` filters
/// `real_dead_code_candidates()`/`real_dead_code_lines()` to `Disposition::Delete` with no
/// re-sort, so the same filter-in-place over the two ALREADY position-joined and (file,
/// line)-ascending committed files (proven by
/// `the_committed_dead_code_json_and_its_lines_sibling_are_position_joined_and_ascending_by_file_
/// then_line` above) reproduces the identical sequence. A `(file, name)` lookup would be wrong for
/// the same reason section 4.3's test avoids one: `src/ingest.rs`'s `ingest_project` is ambiguous.
#[test]
fn the_committed_report_section_6_deletion_list_cites_dead_code_file_line_exactly_as_the_lines_sibling_records_them(
) {
    let report = std::fs::read_to_string(repo_root().join(REPORT_PATH))
        .unwrap_or_else(|e| panic!("{REPORT_PATH} is missing or unreadable ({e})"));
    let citations = section_6_deletion_list_citations(&report);
    let candidates = deserialize_committed_dead_code();
    let lines = deserialize_committed_dead_code_lines();
    assert_eq!(
        candidates.len(),
        lines.len(),
        "{DEAD_CODE_PATH} and {DEAD_CODE_LINES_PATH} must be position-joined (same length) for \
         this filter-in-place comparison to be valid"
    );
    let expected: Vec<(String, String, usize)> = candidates
        .iter()
        .zip(lines.iter())
        .filter(|(c, _)| c.disposition == "delete")
        .map(|(_, l)| (l.file.clone(), l.name.clone(), l.line))
        .collect();
    assert!(
        !expected.is_empty(),
        "zero delete-dispositioned entries in {DEAD_CODE_PATH} - the disposition filter or the \
         committed data is broken"
    );
    assert_eq!(
        citations.len(),
        expected.len(),
        "the report's section 6 deletion list cites {} candidate(s) but {DEAD_CODE_PATH} \
         records {} delete-dispositioned entries - they must list the same candidates in the \
         same order (both come from the same underlying position-joined, filtered sequence)",
        citations.len(),
        expected.len()
    );
    for (i, ((file, name, line), (efile, ename, eline))) in
        citations.iter().zip(expected.iter()).enumerate()
    {
        assert_eq!(
            (file.as_str(), name.as_str()),
            (efile.as_str(), ename.as_str()),
            "deletion-list citation {i} in {REPORT_PATH} is {name} ({file}), but the same-index \
             delete-dispositioned entry in {DEAD_CODE_LINES_PATH} is {ename} ({efile}) - report \
             order and the filtered lines-sibling order have diverged",
        );
        assert_eq!(
            line, eline,
            "report section 6 deletion list cites {name} ({file}:{line}), but \
             {DEAD_CODE_LINES_PATH} records line {eline} for that same candidate",
        );
    }
}
