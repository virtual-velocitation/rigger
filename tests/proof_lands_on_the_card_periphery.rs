//! Periphery (integration) tests for spec 86 criterion 2 (PROOF LANDS ON THE CARD), independent of
//! both the implementer's own fixtures and the existing client-seam test in
//! `proof_row_renders_on_the_card.rs`.
//!
//! Two boundaries neither of those reaches:
//!
//!  - The Done-when, driven end to end through the crate's PUBLIC API (`build_index` ->
//!    `index_events` -> `Projector` -> `dash::card`), from OUTSIDE the crate, over a fixture of its
//!    own. The implementer's own `events.rs` unit test
//!    (`a_product_entity_referenced_by_two_tests_carries_proven_by_2_and_renders_on_its_card`)
//!    already proves this same pipeline, but IN-CRATE and over the implementer's OWN fixture - that
//!    proof rests on the implementer's own understanding of the boundary, exactly the class of gap
//!    this layer exists to close (mirroring `code_entity_test_exclusion_periphery.rs`'s own
//!    precedent for criterion 1's Done-when). A second test here drives the `pending_proof`
//!    forward-reference/reconciliation fold arm (`fold_test_evidence` -> `stage_pending_proof` ->
//!    `reconcile_pending_proof`) through this SAME real pipeline: the implementer's own coverage of
//!    that arm (`sqlite.rs::proof_evidence_c2::an_unresolvable_test_reference_is_staged_and_
//!    reconciled_once_its_definition_later_folds`) hand-builds the two events directly against the
//!    fold, which proves the fold logic is correct IF handed such a sequence but never proves the
//!    real sorted-path pipeline (`project_batches_paced`/`index_events` over `BTreeMap<String,
//!    FileSymbols>`) ever PRODUCES one - a genuinely different fact this test pins by naming its
//!    fixture files so a `tests/`-dir file sorts alphabetically BEFORE the product file it
//!    references. Both tests live in the `symbols` lane only (they drive the real tree-sitter
//!    extraction pass).
//!  - The SERVED `/api/graph?card=<id>` wire contract for the two new `Card` fields. No existing
//!    test crosses the real socket for `card=` at all: the implementer's own `dash.rs` unit tests
//!    call the pure `route`/`card` functions IN-PROCESS (structurally blind to HTTP framing and JSON
//!    serialization), and `proof_row_renders_on_the_card.rs`'s client-seam driver hands `renderCard`
//!    a hand-built JS object directly, never fetching from a server at all - so the serde
//!    `skip_serializing_if` contract (a real JSON KEY present/absent over the wire, not a Rust
//!    struct field's value) and the store's string-encoded `proof_evidence` correctly round-tripping
//!    into a genuine JSON ARRAY (not a double-encoded string) have never been exercised at the true
//!    HTTP boundary until this file. Runs in BOTH lanes (`dash`/`contextgraph` are not feature-gated,
//!    mirroring `dash_kg_graph_route.rs`'s own scope note).
//!  - Round 2's supersession-on-re-extract boundary (three findings from the round-1 review:
//!    `sdet-u86c2-r-fresh-refold-wipes-cross-file-proof`,
//!    `adv-u86c2-r-test-file-re-extraction-double-counts-its-own-unchanged-references`,
//!    `adv-u86c2-r-cross-file-name-match-misattributes-proof-to-the-wrong-entity`). The
//!    implementer's own regression tests for all three
//!    (`sqlite.rs::proof_evidence_c2::re_extracting_an_edited_file_does_not_silently_drop_an_
//!    unrelated_files_accumulated_proof`,
//!    `::re_extracting_a_test_file_supersedes_rather_than_accretes_or_strands_its_own_evidence`,
//!    `::an_ambiguous_same_named_pair_never_gets_confident_credit_through_either_resolution_path`)
//!    call the fold directly with hand-built events (`apply_batch_def`/
//!    `apply_edge_inferred_evidence_fresh`/`apply_code_entity`), which proves the SQL
//!    (`ensure_node`'s `json_patch` merge, `supersede_file_proof`, `resolve_proof_target`'s and
//!    `reconcile_pending_proof`'s uniqueness guards) is correct FOR A SEQUENCE those helpers
//!    assume - never that the real `symbols` extraction pass, re-walking a genuine second-
//!    generation edit on disk, actually PRODUCES that sequence (the same class of gap the first
//!    bullet above closes for round 1). The three tests below drive `build_index` /
//!    `extract_events` / `proof_events` twice each over a real file edit, re-applying only the
//!    ONE file that changed - mirroring what the real replay-key content-hash suppression
//!    (`crate::ingest::key_batch`, driven by `RunCtx::ingest_project_batches` in production) does,
//!    per [`events_for_file`]'s own doc below.

// ---- the Done-when, end to end via the public API (symbols lane only) ----------------------

/// A fixture independent of the implementer's own (`product_fn`/`unused_fn`/`helper`): a product
/// file defining `strike` (referenced by a same-file `#[cfg(test)] mod tests`) and `parry` (defined
/// but never referenced by anything), alongside a separate `tests/`-dir file that also references
/// `strike`.
#[cfg(feature = "symbols")]
const STRIKE_PRODUCT_SRC: &str = "\
fn strike() {}

fn parry() {}

#[cfg(test)]
mod tests {
    use super::strike;

    #[test]
    fn strike_lands() {
        strike();
    }
}
";

#[cfg(feature = "symbols")]
const STRIKE_OUTSIDE_TEST_SRC: &str = "\
#[test]
fn an_outside_combat_test() {
    strike();
}
";

#[cfg(feature = "symbols")]
#[test]
fn proof_lands_through_the_public_pipeline_independent_of_the_implementers_own_fixture() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::{Projection, KIND_FILE};
    use rigger::dash::card;
    use rigger::grounder::symbols::build_index;
    use rigger::grounder::symbols::events::index_events;

    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("combat.rs"), STRIKE_PRODUCT_SRC).unwrap();
    let tests_dir = root.path().join("tests");
    std::fs::create_dir(&tests_dir).unwrap();
    std::fs::write(
        tests_dir.join("combat_periphery.rs"),
        STRIKE_OUTSIDE_TEST_SRC,
    )
    .unwrap();

    let idx = build_index(root.path().to_str().unwrap(), None);
    let mut events = index_events(&idx);
    let p = Projector::open(":memory:", "test").unwrap();
    for (zero_based, event) in events.iter_mut().enumerate() {
        event.position = zero_based as u64 + 1;
        p.apply(event).unwrap();
    }

    let seed_files = [
        "combat.rs".to_string(),
        "tests/combat_periphery.rs".to_string(),
    ];
    let g = p.subgraph(&seed_files, 3).unwrap();

    // PROVEN: strike is referenced by the in-file test (line 11) and the tests/-dir file (line 3) -
    // proven_by: 2, both file:lines, in fold (sorted-path) order.
    let proven = card(&g, "combat.rs::strike").expect("combat.rs::strike is a graph node");
    assert_eq!(
        proven.proven_by, 2,
        "one same-file and one cross-file test-origin reference both prove strike; card: {proven:?}"
    );
    assert_eq!(
        proven.proof_evidence,
        vec![
            "combat.rs:11".to_string(),
            "tests/combat_periphery.rs:3".to_string()
        ],
        "both evidence file:lines land on the card, in fold order; card: {proven:?}"
    );

    // UNREFERENCED: parry, defined in the same file, is never called by any test - the explicit
    // no-test state, never a made-up value.
    let unproven = card(&g, "combat.rs::parry").expect("combat.rs::parry is a graph node");
    assert_eq!(
        unproven.proven_by, 0,
        "parry is never referenced by a test; card: {unproven:?}"
    );
    assert!(unproven.proof_evidence.is_empty());

    // Criterion 1's own promise still holds alongside the new evidence: neither test item ever
    // became a node, and the tests/-dir file still carries no file container node despite
    // contributing evidence.
    let node_ids: std::collections::BTreeSet<&str> =
        g.nodes.iter().map(|n| n.id.as_str()).collect();
    for excluded in [
        "combat.rs::tests",
        "combat.rs::strike_lands",
        "tests/combat_periphery.rs::an_outside_combat_test",
        "tests/combat_periphery.rs",
    ] {
        assert!(
            !node_ids.contains(excluded),
            "{excluded:?} is test code and must never become a graph node; nodes: {node_ids:?}"
        );
    }
    assert!(
        !g.nodes
            .iter()
            .any(|n| n.kind == KIND_FILE && n.id == "tests/combat_periphery.rs"),
        "an all-test file still carries no file container node even though it contributes \
         evidence; nodes: {:?}",
        g.nodes
    );
    assert!(
        !g.edges
            .iter()
            .any(|e| e.from == "tests/combat_periphery.rs"),
        "a test-origin reference never folds an edge of any kind; edges: {:?}",
        g.edges
    );
}

/// A `tests/`-dir file whose path sorts ALPHABETICALLY BEFORE the product file it references
/// (`tests/aaa_check.rs` < `zzz_product.rs`), so the real sorted-path pipeline
/// (`project_batches_paced`/`index_events` over `BTreeMap<String, FileSymbols>`) folds this
/// evidence event BEFORE `finisher`'s own definition exists - the forward-reference case
/// `pending_proof`/`reconcile_pending_proof` exists for, produced here by the REAL pipeline rather
/// than a hand-built event sequence.
#[cfg(feature = "symbols")]
const FINISHER_PRODUCT_SRC: &str = "fn finisher() {}\n";

#[cfg(feature = "symbols")]
const FORWARD_CHECK_SRC: &str = "\
#[test]
fn checks_finisher() {
    finisher();
}
";

#[cfg(feature = "symbols")]
#[test]
fn forward_referenced_evidence_through_the_public_pipeline_is_reconciled_once_its_definition_later_folds(
) {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::Projection;
    use rigger::dash::card;
    use rigger::grounder::symbols::build_index;
    use rigger::grounder::symbols::events::index_events;

    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("zzz_product.rs"), FINISHER_PRODUCT_SRC).unwrap();
    let tests_dir = root.path().join("tests");
    std::fs::create_dir(&tests_dir).unwrap();
    std::fs::write(tests_dir.join("aaa_check.rs"), FORWARD_CHECK_SRC).unwrap();

    let idx = build_index(root.path().to_str().unwrap(), None);
    assert!(
        idx.files().keys().next().map(String::as_str) == Some("tests/aaa_check.rs"),
        "fixture precondition: the tests/-dir file must sort before the product file so this test \
         actually exercises the forward-reference path, not the same-order case round 1 already \
         covers; files: {:?}",
        idx.files().keys().collect::<Vec<_>>()
    );

    let mut events = index_events(&idx);
    let p = Projector::open(":memory:", "test").unwrap();
    for (zero_based, event) in events.iter_mut().enumerate() {
        event.position = zero_based as u64 + 1;
        p.apply(event).unwrap();
    }

    let g = p
        .subgraph(
            &[
                "zzz_product.rs".to_string(),
                "tests/aaa_check.rs".to_string(),
            ],
            3,
        )
        .unwrap();
    let finisher =
        card(&g, "zzz_product.rs::finisher").expect("zzz_product.rs::finisher is a graph node");
    assert_eq!(
        finisher.proven_by, 1,
        "the forward-referenced evidence is reconciled onto the definition once it folds, even \
         though it arrived first through the real pipeline; card: {finisher:?}"
    );
    assert_eq!(
        finisher.proof_evidence,
        vec!["tests/aaa_check.rs:3".to_string()]
    );
}

// ---- round 2: proof survives a REAL re-extraction, through the SAME public pipeline --------

/// Write `contents` to `root/rel`, creating any parent directory (`tests/`, mainly) first, and
/// overwriting a prior generation the same way a real edit does. The small filesystem-fixture
/// helper every round-2 test below shares.
#[cfg(feature = "symbols")]
fn root_write(root: &tempfile::TempDir, rel: &str, contents: &str) {
    let path = root.path().join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

/// One file's own event batch at a given `SymbolIndex` snapshot - the SAME composition
/// `index_events`/`project_batches_paced` use for every file (structural events, then this
/// file's own test-origin evidence via `proof_events`), but scoped to ONE named file so a
/// round-2 re-extraction can be simulated by feeding only the file that actually changed. That
/// is exactly what the real replay-key content-hash suppression accomplishes in production
/// (`crate::ingest::key_batch` keys a file's WHOLE batch on its content hash; `RunCtx::
/// ingest_project_batches`'s sink appends a batch only when its key set is new, so an unchanged
/// file's batch is never handed to the fold again on a later run while a changed file's whole
/// batch re-applies) - this helper reproduces that SELECTION at the test level without needing a
/// live `RunCtx`/eventstore, while `extract_events`/`proof_events` themselves stay the real,
/// public, tree-sitter-backed extraction the production walk uses, never a hand-built event.
#[cfg(feature = "symbols")]
fn events_for_file(
    idx: &rigger::grounder::symbols::model::SymbolIndex,
    path: &str,
) -> Vec<rigger::eventstore::Event> {
    use rigger::grounder::symbols::events::{extract_events, proof_events};
    let fs = idx
        .files()
        .get(path)
        .unwrap_or_else(|| panic!("fixture precondition: {path:?} is in the index"));
    let mut events = extract_events(path, fs);
    events.extend(proof_events(path, fs));
    events
}

/// Apply `events` to `p`, continuing the position sequence from `*next_position` (advancing it
/// past every event applied), so several real per-file batches - an initial ingest, then a later
/// re-extraction of just the file that changed - compose onto ONE `Projector` exactly as a real
/// multi-generation run would append them, never restarting the position sequence a fresh `Vec`
/// per round would otherwise imply.
#[cfg(feature = "symbols")]
fn apply_events(
    p: &rigger::contextgraph::sqlite::Projector,
    events: &mut [rigger::eventstore::Event],
    next_position: &mut u64,
) {
    use rigger::contextgraph::Projection;
    for event in events.iter_mut() {
        event.position = *next_position;
        *next_position += 1;
        p.apply(event).unwrap();
    }
}

/// Re-read `root`'s CURRENT on-disk state (a real, freshly built `SymbolIndex` - so a caller
/// that just edited `path` on disk gets that edit's own real extraction, never a stale one),
/// then apply ONLY `path`'s own batch to `p` - the single "one file (re-)extracts through the
/// real pipeline and folds" step every round-2 test below performs one or more times, over
/// [`events_for_file`] and [`apply_events`] together.
#[cfg(feature = "symbols")]
fn reextract_file(
    root: &tempfile::TempDir,
    p: &rigger::contextgraph::sqlite::Projector,
    path: &str,
    next_position: &mut u64,
) {
    let idx = rigger::grounder::symbols::build_index(root.path().to_str().unwrap(), None);
    apply_events(p, &mut events_for_file(&idx, path), next_position);
}

/// Fixture for [`cross_file_proof_survives_a_real_reextraction_of_the_defining_file`]: `watch`
/// (proven by a cross-file test) and `idle` (never referenced) - independent of both the
/// implementer's own `product.rs`/`product_fn` fixture and this file's own `combat.rs`/`strike`
/// fixture above.
#[cfg(feature = "symbols")]
const SENTINEL_V1_SRC: &str = "fn watch() {}\n\nfn idle() {}\n";

/// `sentinel.rs` after an edit UNRELATED to `watch` (round 2,
/// `sdet-u86c2-r-fresh-refold-wipes-cross-file-proof`): a new function is appended, changing the
/// file's content/hash so it genuinely re-extracts fresh, while `watch`'s own line (1) is
/// untouched - the exact "editing ANY line of product.rs" shape the finding names, reproduced
/// with a real edit on disk rather than a synthetic `fresh: true` flag on a hand-built event.
#[cfg(feature = "symbols")]
const SENTINEL_V2_SRC: &str = "fn watch() {}\n\nfn idle() {}\n\nfn alert() {}\n";

#[cfg(feature = "symbols")]
const SENTINEL_TEST_SRC: &str = "\
#[test]
fn checks_watch() {
    watch();
}
";

/// Closes `sdet-u86c2-r-fresh-refold-wipes-cross-file-proof` at the periphery: drives the real
/// pipeline twice over a genuine file edit. A cross-file test first proves `watch`; `sentinel.rs`
/// is then edited (unrelated to `watch`) and re-extracted ALONE - the untouched test file's batch
/// is never re-applied, mirroring what real content-hash suppression would do in production (see
/// [`events_for_file`]'s doc) - and `watch`'s proof must survive the re-fold.
#[cfg(feature = "symbols")]
#[test]
fn cross_file_proof_survives_a_real_reextraction_of_the_defining_file() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::Projection;
    use rigger::dash::card;

    let root = tempfile::tempdir().unwrap();
    root_write(&root, "sentinel.rs", SENTINEL_V1_SRC);
    root_write(&root, "tests/sentinel_periphery.rs", SENTINEL_TEST_SRC);

    let p = Projector::open(":memory:", "test").unwrap();
    let mut next_position = 1u64;
    reextract_file(&root, &p, "sentinel.rs", &mut next_position);
    reextract_file(&root, &p, "tests/sentinel_periphery.rs", &mut next_position);

    let baseline = p.subgraph(&["sentinel.rs".to_string()], 2).unwrap();
    let watch = card(&baseline, "sentinel.rs::watch").expect("sentinel.rs::watch is a graph node");
    assert_eq!(
        watch.proven_by, 1,
        "sanity: the cross-file test proves watch before any re-extraction; card: {watch:?}"
    );
    assert_eq!(
        watch.proof_evidence,
        vec!["tests/sentinel_periphery.rs:3".to_string()]
    );

    // sentinel.rs is edited (unrelated to watch) and re-extracts ALONE; the test file is
    // untouched and its batch is never re-applied here.
    root_write(&root, "sentinel.rs", SENTINEL_V2_SRC);
    reextract_file(&root, &p, "sentinel.rs", &mut next_position);

    let after = p.subgraph(&["sentinel.rs".to_string()], 2).unwrap();
    let watch_after =
        card(&after, "sentinel.rs::watch").expect("sentinel.rs::watch survives the re-extraction");
    assert_eq!(
        watch_after.proven_by, 1,
        "re-extracting sentinel.rs must not silently drop the proof a DIFFERENT, untouched file \
         already established; card: {watch_after:?}"
    );
    assert_eq!(
        watch_after.proof_evidence,
        vec!["tests/sentinel_periphery.rs:3".to_string()],
        "the evidence entry itself must survive too; card: {watch_after:?}"
    );
    let idle_after = card(&after, "sentinel.rs::idle").expect("sentinel.rs::idle is a graph node");
    assert_eq!(
        idle_after.proven_by, 0,
        "idle is never referenced by any test"
    );
    let alert_after = card(&after, "sentinel.rs::alert")
        .expect("the newly added alert folded structurally alongside watch/idle");
    assert_eq!(alert_after.proven_by, 0);
}

#[cfg(feature = "symbols")]
const DUO_SRC: &str = "fn left() {}\n\nfn right() {}\n";

#[cfg(feature = "symbols")]
const DUO_TEST_V1_SRC: &str = "\
#[test]
fn checks_left() {
    left();
}
";

/// An unrelated line is added ABOVE the test (shifts the reference's own line from 3 to 4) while
/// still referencing `left` - this step proves a re-extraction NETS TO THE SAME evidence (one
/// entry, at the NEW line), never a second, stale entry lingering at the old one.
#[cfg(feature = "symbols")]
const DUO_TEST_V2_SRC: &str = "\
// housekeeping
#[test]
fn checks_left() {
    left();
}
";

/// The test is edited to reference `right` instead of `left` entirely - proof must move WHOLE,
/// never accrete on `right` while stranding a stale entry on `left`.
#[cfg(feature = "symbols")]
const DUO_TEST_V3_SRC: &str = "\
#[test]
fn checks_right() {
    right();
}
";

/// Closes `adv-u86c2-r-test-file-re-extraction-double-counts-its-own-unchanged-references` at
/// the periphery. Three real edits to the SAME test file, re-extracted through the real pipeline
/// each time: an edit that only shifts the reference's own line (must net to ONE entry, not
/// two), then an edit that swaps which product entity it reaches entirely (proof must move
/// whole, never strand a stale entry on the old target).
#[cfg(feature = "symbols")]
#[test]
fn a_real_reextraction_of_a_test_file_supersedes_rather_than_accretes_or_strands_its_own_evidence()
{
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::Projection;
    use rigger::dash::card;

    // One step of the walk below: `tests/duo_periphery.rs` is (re-)written to `src` and
    // re-extracted fresh, and `left`/`right` must land at exactly `want_left`/`want_right`
    // (with `left_evidence` naming the single surviving entry, when any) afterward.
    struct Step {
        src: &'static str,
        want_left: usize,
        want_right: usize,
        left_evidence: Option<&'static str>,
    }
    let walk = [
        Step {
            src: DUO_TEST_V1_SRC,
            want_left: 1,
            want_right: 0,
            left_evidence: Some("tests/duo_periphery.rs:3"),
        },
        // The reference's own line shifts (an unrelated line added above it); same target -
        // must net to ONE entry at the NEW line, never a second, stale one at the old line.
        Step {
            src: DUO_TEST_V2_SRC,
            want_left: 1,
            want_right: 0,
            left_evidence: Some("tests/duo_periphery.rs:4"),
        },
        // The test is edited to reference `right` instead - proof must move WHOLE.
        Step {
            src: DUO_TEST_V3_SRC,
            want_left: 0,
            want_right: 1,
            left_evidence: None,
        },
    ];

    let root = tempfile::tempdir().unwrap();
    root_write(&root, "duo.rs", DUO_SRC);
    let p = Projector::open(":memory:", "test").unwrap();
    let mut next_position = 1u64;
    reextract_file(&root, &p, "duo.rs", &mut next_position);

    for (step_idx, step) in walk.iter().enumerate() {
        root_write(&root, "tests/duo_periphery.rs", step.src);
        reextract_file(&root, &p, "tests/duo_periphery.rs", &mut next_position);

        let g = p.subgraph(&["duo.rs".to_string()], 2).unwrap();
        let left = card(&g, "duo.rs::left").expect("duo.rs::left is a graph node");
        assert_eq!(
            left.proven_by, step.want_left,
            "step {step_idx}: duo.rs::left; card: {left:?}"
        );
        assert_eq!(
            card(&g, "duo.rs::right")
                .expect("duo.rs::right is a graph node")
                .proven_by,
            step.want_right,
            "step {step_idx}: duo.rs::right"
        );
        if let Some(evidence) = step.left_evidence {
            assert_eq!(
                left.proof_evidence,
                vec![evidence.to_string()],
                "step {step_idx}: duo.rs::left's evidence must be exactly this one entry, \
                 never accreted or left stale; card: {left:?}"
            );
        }
    }
}

#[cfg(feature = "symbols")]
const ALPHA_V1_SRC: &str = "fn shared() {}\n";

/// `alpha.rs` after an edit UNRELATED to `shared` - adds a second function so the file
/// genuinely re-extracts fresh, refiring `TYPE_CODE_ENTITY_EXTRACTED` for `shared` and, with it,
/// `reconcile_pending_proof`'s own ambiguity guard.
#[cfg(feature = "symbols")]
const ALPHA_V2_SRC: &str = "fn shared() {}\n\nfn alpha_only() {}\n";

#[cfg(feature = "symbols")]
const BETA_SRC: &str = "fn shared() {}\n";

#[cfg(feature = "symbols")]
const AMBIGUOUS_TEST_SRC: &str = "\
#[test]
fn checks_shared() {
    shared();
}
";

/// Closes `adv-u86c2-r-cross-file-name-match-misattributes-proof-to-the-wrong-entity` at the
/// periphery, over BOTH resolution paths the finding names. `alpha.rs`/`beta.rs` both define
/// `shared`; a third file references it with no local definition of its own. First proves the
/// RESOLVED path (the reference folds after both real definitions already exist, through the
/// real sorted-path pipeline - alphabetical file order is exactly what makes a `LIMIT 1`-style
/// pick deterministic-but-wrong in a REAL tree, not merely a contrived one); then edits
/// `alpha.rs` elsewhere and re-extracts it ALONE, re-firing `reconcile_pending_proof` on the
/// still-pending, still-ambiguous evidence - the SECOND path the finding names.
#[cfg(feature = "symbols")]
#[test]
fn a_real_ambiguous_same_named_pair_never_gets_confident_credit_through_either_resolution_path() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::Projection;
    use rigger::dash::card;
    use rigger::grounder::symbols::build_index;
    use rigger::grounder::symbols::events::index_events;

    let root = tempfile::tempdir().unwrap();
    root_write(&root, "alpha.rs", ALPHA_V1_SRC);
    root_write(&root, "beta.rs", BETA_SRC);
    root_write(&root, "tests/ambiguous_periphery.rs", AMBIGUOUS_TEST_SRC);

    let idx1 = build_index(root.path().to_str().unwrap(), None);
    assert_eq!(
        idx1.files().keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["alpha.rs", "beta.rs", "tests/ambiguous_periphery.rs"],
        "fixture precondition: sorted order must fold both definitions before the reference, so \
         this test genuinely exercises the RESOLVED cross-file path, not the pending/forward one"
    );

    let p = Projector::open(":memory:", "test").unwrap();
    let mut next_position = 1u64;
    apply_events(&p, &mut index_events(&idx1), &mut next_position);

    let seeds = ["alpha.rs".to_string(), "beta.rs".to_string()];
    let before = p.subgraph(&seeds, 2).unwrap();
    assert_eq!(
        card(&before, "alpha.rs::shared")
            .expect("alpha.rs::shared is a graph node")
            .proven_by,
        0,
        "an ambiguous name must not confidently credit alpha.rs's definition"
    );
    assert_eq!(
        card(&before, "beta.rs::shared")
            .expect("beta.rs::shared is a graph node")
            .proven_by,
        0,
        "an ambiguous name must not confidently credit beta.rs's definition either"
    );

    // The SECOND path: alpha.rs is edited elsewhere (unrelated to shared) and re-extracts ALONE.
    root_write(&root, "alpha.rs", ALPHA_V2_SRC);
    reextract_file(&root, &p, "alpha.rs", &mut next_position);
    let after = p.subgraph(&seeds, 2).unwrap();
    assert_eq!(
        card(&after, "alpha.rs::shared")
            .expect("alpha.rs::shared is a graph node")
            .proven_by,
        0,
        "a re-fold must not retroactively claim the still-ambiguous pending evidence via \
         reconcile_pending_proof merely because it is the one refolding right now"
    );
    assert_eq!(
        card(&after, "beta.rs::shared")
            .expect("beta.rs::shared is a graph node")
            .proven_by,
        0
    );
}

#[cfg(feature = "symbols")]
const GONE_PRODUCT_SRC: &str = "fn vanish() {}\n";

#[cfg(feature = "symbols")]
const GONE_TEST_V1_SRC: &str = "\
#[test]
fn checks_vanish() {
    vanish();
}
";

/// The test file after its ONLY reference is deleted - replaced with a plain comment, ZERO
/// references left in the file (round 3, adv-u86c2-r2-deleted-test-reference-strands-proof-
/// forever): deleting or rewriting the one test that proved something is a routine, ordinary
/// test-suite operation, not an edge case.
#[cfg(feature = "symbols")]
const GONE_TEST_V2_SRC: &str = "// the test that proved vanish() was deleted\n";

/// Closes `adv-u86c2-r2-deleted-test-reference-strands-proof-forever` at the periphery: a product
/// function proven by exactly one test, then that test's ONLY reference deleted (not replaced with
/// a DIFFERENT reference - the file's own evidence set drops to ZERO, never merely changes), and
/// the test file re-extracted ALONE through the real pipeline. Before round 3, `proof_events`
/// returned an empty `Vec` whenever a file's evidence set was empty; combined with `extract_events`
/// ALSO being empty for this `tests/`-dir file, `events_for_file`'s own combined batch was itself
/// empty, so nothing was ever applied for this step and `fold_test_evidence`/`supersede_file_proof`
/// never ran - the stale proof stood forever. Round 3 makes an empty evidence set a first-class
/// boundary (mirroring criterion 3's own empty-after-exclusion pattern for the structural side): the
/// file still emits exactly one sentinel event, so the fold still retracts its own stale
/// contribution.
#[cfg(feature = "symbols")]
#[test]
fn a_deleted_test_reference_retracts_its_stale_proof_through_the_real_pipeline() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::Projection;
    use rigger::dash::card;

    let root = tempfile::tempdir().unwrap();
    root_write(&root, "gone.rs", GONE_PRODUCT_SRC);
    root_write(&root, "tests/gone_periphery.rs", GONE_TEST_V1_SRC);

    let p = Projector::open(":memory:", "test").unwrap();
    let mut next_position = 1u64;
    reextract_file(&root, &p, "gone.rs", &mut next_position);
    reextract_file(&root, &p, "tests/gone_periphery.rs", &mut next_position);

    let baseline = p.subgraph(&["gone.rs".to_string()], 2).unwrap();
    let vanish = card(&baseline, "gone.rs::vanish").expect("gone.rs::vanish is a graph node");
    assert_eq!(
        vanish.proven_by, 1,
        "sanity: the test proves vanish before its reference is deleted; card: {vanish:?}"
    );
    assert_eq!(
        vanish.proof_evidence,
        vec!["tests/gone_periphery.rs:3".to_string()]
    );

    // The test's ONLY reference is deleted (replaced with a plain comment); the test file
    // re-extracts ALONE, its evidence set now EMPTY.
    root_write(&root, "tests/gone_periphery.rs", GONE_TEST_V2_SRC);
    reextract_file(&root, &p, "tests/gone_periphery.rs", &mut next_position);

    let after = p.subgraph(&["gone.rs".to_string()], 2).unwrap();
    let vanish_after =
        card(&after, "gone.rs::vanish").expect("gone.rs::vanish still exists as a graph node");
    assert_eq!(
        vanish_after.proven_by, 0,
        "a deleted test reference must retract its own stale proof, not strand it forever; card: \
         {vanish_after:?}"
    );
    assert!(
        vanish_after.proof_evidence.is_empty(),
        "the stale file:line entry must be retracted, not merely left uncounted; card: \
         {vanish_after:?}"
    );
}

// ---- the SERVED /api/graph?card= wire contract (both lanes) --------------------------------

/// Start `serve` on a fresh ephemeral loopback port, fetch `GET <path>` once against a fixture-graph
/// provider, and return the raw HTTP response - or `None` on a genuine socket-level failure. Mirrors
/// `dash_kg_graph_route.rs::try_fetch_served` verbatim - the established per-file duplication
/// convention for this class of served-route periphery test in this codebase (each `tests/*.rs`
/// integration test compiles as its own independent crate, so the harness plumbing cannot be shared
/// via a plain `use`). See that file's own doc for why the listener is handed to `serve_on` rather
/// than dropped and re-bound.
fn try_fetch_served(path: &str, graph: rigger::contextgraph::Graph) -> Option<String> {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::time::{Duration, Instant};

    let listener = TcpListener::bind(("127.0.0.1", 0)).ok()?;
    let addr = listener.local_addr().ok()?;

    let graph_provider = {
        let graph = graph.clone();
        move |_instance: Option<&str>| -> rigger::contextgraph::Graph { graph.clone() }
    };
    let provider = move |_instance: Option<&str>| -> Result<rigger::dash::DashInputs, String> {
        Ok((
            Vec::new(),
            graph.clone(),
            Vec::new(),
            std::collections::HashMap::new(),
        ))
    };
    let calls_provider =
        |_: Option<&str>, _: &[String], _: rigger::contextgraph::Direction, _: i64, _: &str| {
            rigger::contextgraph::CallGraph::default()
        };
    let instances_provider = Vec::new;
    std::thread::spawn(move || {
        let _ = rigger::dash::serve_on(
            listener,
            provider,
            graph_provider,
            calls_provider,
            instances_provider,
            3,
            "rigger-run",
            "origin/main",
        );
    });

    let deadline = Instant::now() + Duration::from_millis(1500);
    let mut client = loop {
        match TcpStream::connect(addr) {
            Ok(s) => break s,
            Err(_) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return None,
        }
    };

    let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n");
    if client.write_all(req.as_bytes()).is_err() {
        return None;
    }
    let mut resp = String::new();
    match client.read_to_string(&mut resp) {
        Ok(_) => Some(resp),
        Err(_) => None,
    }
}

/// Drive the hand-rolled dash server over a REAL loopback socket and fetch `GET <path>`, retrying on
/// a socket-level transient. Mirrors `dash_kg_graph_route.rs::fetch_served` verbatim.
fn fetch_served(path: &str, graph: &rigger::contextgraph::Graph) -> String {
    for _ in 0..200 {
        if let Some(resp) = try_fetch_served(path, graph.clone()) {
            return resp;
        }
    }
    panic!(
        "the dash server never served {path} over the real socket after many fresh-port attempts"
    );
}

/// Split a raw HTTP response into its body (everything past the header terminator). Mirrors
/// `dash_kg_graph_route.rs::body_of` verbatim.
fn body_of(resp: &str) -> &str {
    resp.split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .expect("a served response body")
}

/// A minimal graph with one PROVEN code entity (`proven_by`/`proof_evidence` attrs already folded)
/// and one UNPROVEN one (neither attr present) - built by hand, mirroring how the real fold leaves
/// them (a decimal-digit string and a JSON-array-shaped string respectively; see
/// `contextgraph::sqlite::record_proof`'s own doc for why), so this test is scoped to the SERVED
/// wire contract, independent of whether the fold itself is correct (round 1/2 above already prove
/// that through the real pipeline).
fn proof_fixture_graph() -> rigger::contextgraph::Graph {
    use rigger::contextgraph::{Graph, Node, KIND_CODE_ENTITY};
    let node = |id: &str, attrs: &[(&str, &str)]| Node {
        id: id.to_string(),
        kind: KIND_CODE_ENTITY.to_string(),
        attrs: attrs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    };
    Graph {
        nodes: vec![
            node(
                "widget.rs::sturdy",
                &[
                    ("name", "sturdy"),
                    ("proven_by", "2"),
                    (
                        "proof_evidence",
                        r#"["widget.rs:5","tests/widget_periphery.rs:2"]"#,
                    ),
                ],
            ),
            node("widget.rs::fragile", &[("name", "fragile")]),
        ],
        edges: Vec::new(),
    }
}

/// The SERVED `/api/graph?card=<id>` route carries `proven_by`/`proof_evidence` over the REAL socket
/// as genuine JSON - `proof_evidence` a real JSON ARRAY (not the double-encoded string the store
/// holds it as internally), `proven_by` a real JSON number. Guards the serve/route/serialize seam
/// the in-process `dash.rs` unit tests and the JS-mocked client-seam test are both blind to (neither
/// ever sends this struct through `serde_json` onto a real socket).
#[test]
fn the_served_graph_card_route_carries_proven_by_and_proof_evidence_as_a_real_json_array() {
    let graph = proof_fixture_graph();
    let resp = fetch_served("/api/graph?card=widget.rs::sturdy", &graph);
    assert!(
        resp.starts_with("HTTP/1.1 200 OK"),
        "GET /api/graph?card= returns 200 over the real serve socket:\n{resp}"
    );
    let json: serde_json::Value =
        serde_json::from_str(body_of(&resp)).expect("the served card body is valid JSON");
    assert_eq!(
        json["card"]["proven_by"], 2,
        "proven_by crosses the real socket as a JSON number: {json}"
    );
    assert_eq!(
        json["card"]["proof_evidence"],
        serde_json::json!(["widget.rs:5", "tests/widget_periphery.rs:2"]),
        "proof_evidence crosses the real socket as a genuine JSON array, not a double-encoded \
         string: {json}"
    );
}

/// The SERVED `/api/graph?card=<id>` route OMITS the `proof_evidence` key entirely for an unproven
/// entity (the real `skip_serializing_if` contract firing over the wire, never observable from a
/// Rust struct field's value alone) while still serving `proven_by: 0` - so the client can render
/// the explicit "no test reaches this entity" state from a REAL present zero, never an absent field.
#[test]
fn the_served_graph_card_route_omits_proof_evidence_but_still_serves_proven_by_zero_for_an_unproven_entity(
) {
    let graph = proof_fixture_graph();
    let resp = fetch_served("/api/graph?card=widget.rs::fragile", &graph);
    assert!(
        resp.starts_with("HTTP/1.1 200 OK"),
        "GET /api/graph?card= returns 200 over the real serve socket:\n{resp}"
    );
    let body = body_of(&resp);
    assert!(
        !body.contains("proof_evidence"),
        "an unproven entity's proof_evidence is OMITTED from the wire (skip_serializing_if), not \
         sent as an empty array: {body}"
    );
    let json: serde_json::Value =
        serde_json::from_str(body).expect("the served card body is valid JSON");
    assert_eq!(
        json["card"]["proven_by"], 0,
        "proven_by is ALWAYS present (never omitted), so the client can render the explicit \
         no-test state from a real 0, not an absent field: {json}"
    );
}
