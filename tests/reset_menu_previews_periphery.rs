//! PERIPHERY (contract / API) tests for spec 68, criterion 3 - the two READ-ONLY preview
//! primitives the bare-menu report is built on: `contextgraph::sqlite::Projector::count_prunable`
//! and `eventstore::sqlite::Store::count_derived_duplicates`.
//!
//! `tests/reset_menu.rs` and `tests/reset_derived_compaction.rs` already drive the compiled
//! binary end-to-end and prove the printed menu lines agree with a real flagged prune - but both
//! fixtures exercise only ONE dimension of `PruneStats` (dead-run nodes) and only ONE derived
//! type's duplicate count, read back as a substring of the binary's stdout. This file drives the
//! two new `pub fn`s DIRECTLY at the library's public surface, to close what a subprocess/stdout
//! comparison cannot reach precisely:
//!
//!   1. **The EDGES dimension.** No existing test ever seeds a superseded structural edge and
//!      checks `count_prunable`'s `superseded_edges` field - the fixture pattern that would prove
//!      it already exists (`tests/graph_superseded_prune.rs`, spec 41) but was never extended to
//!      the new read-only twin. This proves `count_prunable` reports the SAME edge count `prune`
//!      then actually reclaims, not only the node count, and that asking twice never consumes
//!      anything the real prune would then miss.
//!   2. **Project scoping on a shared backend.** `count_prunable`'s own doc promises it is
//!      "scoped to self.project exactly like prune - a shared backend never counts another
//!      project's same-id node or edge" - a promise no existing test exercises with two projects
//!      sharing one graph.db file.
//!   3. **Per-type parity and order.** `count_derived_duplicates` returns a `Vec<(String, usize)>`
//!      in `identity.types()` order, zeros included for an untouched type - the CLI tests check
//!      only a sum across one seeded type. This proves the per-type breakdown matches
//!      `prune_derived_index`'s own `PrunedDerived::removed` exactly, across multiple types with
//!      different counts.
//!   4. **The narrower read contract.** `count_derived_duplicates` needs no
//!      `ContentIdentity::with_reasserting_types` declaration (its own doc: "a count writes
//!      nothing, so the one input that check guards against getting wrong is not read here at
//!      all"), unlike `prune_derived_index`, which refuses an undeclared policy. This proves the
//!      preview succeeds on exactly the identity value the real prune would refuse.
//!   5. **Literal namespace matching, not a wildcard.** `count_derived_duplicates`'s own prefix
//!      match (`substr(stream, 1, length(?2)) = ?2`) is a NEW query, textually similar to but
//!      independent of `prune_derived_index`'s - proven here not to have regressed into a
//!      `LIKE`-style match that would sweep in a neighbor namespace whose prefix merely looks
//!      like a pattern (mirrors the same concern `tests/reset_derived_compaction_periphery.rs`
//!      already proves for the delete path, now proven for this new read path independently).

use rigger::contextgraph::sqlite::{Projector, PruneStats};
use rigger::contextgraph::Projection;
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{ContentIdentity, Event, EventStore, ExpectedRevision};
use std::time::{Duration, UNIX_EPOCH};

// ---------------------------------------------------------------------------------------
// Harness (mirrors tests/graph_superseded_prune.rs and tests/reset_derived_compaction_periphery.rs;
// each integration suite is its own binary, so a small harness is duplicated per file by this
// codebase's existing convention).
// ---------------------------------------------------------------------------------------

/// Fold a `CodeEntityExtracted` (`file` defines `name`) from its raw on-log JSON at `pos`. `fresh`
/// marks the FIRST event of an extraction batch, whose fold supersedes the file's prior live
/// structural edges before folding the new batch (mirrors `graph_superseded_prune.rs::apply_def`).
fn apply_def(p: &Projector, pos: u64, file: &str, name: &str, line: u32, fresh: bool, secs: u64) {
    let payload = serde_json::json!({
        "file": file, "name": name, "kind": "function", "line": line, "lang": "rust",
        "fresh": fresh,
    });
    let mut e = Event::new(
        rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
        serde_json::to_vec(&payload).unwrap(),
    )
    .with_valid_from(UNIX_EPOCH + Duration::from_secs(secs));
    e.position = pos;
    p.apply(&e).unwrap();
}

/// The nanosecond boundary an edge carries for a fact retired `secs` after the epoch - the same
/// time base `valid_to` is stored in.
fn nanos(secs: u64) -> i64 {
    Duration::from_secs(secs).as_nanos() as i64
}

fn keyed(type_: &str, data: Vec<u8>, key: &str, secs: u64) -> Event {
    Event::new(type_, data)
        .with_meta(rigger::ingest::META_REPLAY_KEY, key)
        .with_valid_from(UNIX_EPOCH + Duration::from_secs(secs))
}

fn code_entity() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "file": "src/a.rs", "name": "alpha", "kind": "function", "line": 1, "lang": "rust",
    }))
    .unwrap()
}

fn edge_inferred() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({ "file": "src/a.rs", "name": "beta", "lang": "rust" }))
        .unwrap()
}

const KEY_CODE: &str = "gc/src/a.rs@h1#0";
const KEY_EDGE: &str = "gc/src/a.rs@h1#1";
/// How many times each key is re-recorded. `- 1` of each is the prunable duplicate count.
const CODE_ROUNDS: usize = 4;
const EDGE_ROUNDS: usize = 3;

// ---------------------------------------------------------------------------------------
// Projector::count_prunable
// ---------------------------------------------------------------------------------------

#[test]
fn count_prunable_reports_the_same_nodes_and_superseded_edges_a_real_prune_then_removes() {
    let p = Projector::open(":memory:", "test").unwrap();
    let file = "src/a.rs";

    // The exact three-run re-extraction shape `graph_superseded_prune.rs` proves the edge count
    // against, extended with a `bar` node that is never re-extracted after run 1 - a dead node a
    // real `--runs` would also drop.
    apply_def(&p, 1, file, "foo", 5, true, 100);
    apply_def(&p, 2, file, "bar", 9, false, 100);
    apply_def(&p, 10, file, "foo", 12, true, 200);
    apply_def(&p, 20, file, "foo", 3, true, 300);
    apply_def(&p, 21, file, "baz", 7, false, 300);

    let boundary = nanos(300);
    // `bar`'s own CONTAINS edge is BOTH touched by the node drop (its to_id is the dropped node)
    // AND independently retired before the boundary - the overlap `prune`'s node-cascade delete
    // consumes first, so it must NOT be double-counted under `superseded_edges` too. Only `foo`'s
    // superseded edge (which touches no dropped node) is left for the boundary predicate alone.
    let drop = vec!["src/a.rs::bar".to_string()];

    // Read-only and idempotent: asking twice reports the same thing, because nothing was touched.
    let first = p.count_prunable(&drop, Some(boundary)).unwrap();
    let second = p.count_prunable(&drop, Some(boundary)).unwrap();
    assert_eq!(
        first, second,
        "a read-only preview must report the same count on repeat asks"
    );
    assert_eq!(
        first,
        PruneStats {
            nodes: 1,
            superseded_edges: 1,
        },
        "count_prunable must report the orphaned bar node and ONLY foo's superseded edge - bar's \
         own superseded edge touches the dropped node and must not be double-counted alongside it \
         (a real prune's node-cascade delete removes it before the boundary delete ever runs)"
    );

    // The real prune then removes EXACTLY what the preview counted - never more, never less.
    let removed = p.prune(&drop, Some(boundary)).unwrap();
    assert_eq!(
        removed, first,
        "a real prune must remove EXACTLY the counts its own read-only preview reported"
    );

    // And the preview now agrees the graph is clean at this same drop set and boundary.
    let after = p.count_prunable(&drop, Some(boundary)).unwrap();
    assert_eq!(
        after,
        PruneStats::default(),
        "re-previewing the same drop set and boundary after a real prune must report nothing left"
    );
}

#[test]
fn count_prunable_is_scoped_to_its_own_project_on_a_shared_backend() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("shared_graph.db");
    let path = db.to_str().unwrap();

    // Two projects, ONE backend file - the shape a shared backend is documented to serve. Both
    // fold the SAME file/entity names and the SAME re-extraction shape, so a node id or an edge
    // boundary that leaked across the project column would double-count or cross-prune.
    let a = Projector::open(path, "proj-a").unwrap();
    let b = Projector::open(path, "proj-b").unwrap();
    // `applied` (the fold's idempotency ledger) keys ONLY on position - the SAME meaning a global
    // event-log position carries in production, unique across every project a backend holds - so
    // the two projects' synthetic positions here must not overlap, or the second project's folds
    // would be skipped as already-applied duplicates of the first's.
    for (p, base) in [(&a, 0u64), (&b, 100u64)] {
        apply_def(p, base + 1, "src/a.rs", "foo", 5, true, 100);
        apply_def(p, base + 2, "src/a.rs", "bar", 9, false, 100);
        apply_def(p, base + 10, "src/a.rs", "foo", 12, true, 200);
        apply_def(p, base + 20, "src/a.rs", "foo", 3, true, 300);
        apply_def(p, base + 21, "src/a.rs", "baz", 7, false, 300);
    }
    let boundary = nanos(300);
    let drop = vec!["src/a.rs::bar".to_string()];

    let stats_a = a.count_prunable(&drop, Some(boundary)).unwrap();
    let stats_b = b.count_prunable(&drop, Some(boundary)).unwrap();
    let expected = PruneStats {
        nodes: 1,
        superseded_edges: 1,
    };
    assert_eq!(
        stats_a, expected,
        "proj-a's own preview must count only its own node and edges"
    );
    assert_eq!(
        stats_b, expected,
        "proj-b, seeded identically to proj-a with the same ids, must independently report the \
         SAME counts from its own rows - neither project's preview may see the other's"
    );

    // Pruning proj-a must never move proj-b's numbers: same id, same backend file, different
    // project column.
    a.prune(&drop, Some(boundary)).unwrap();
    let stats_b_after = b.count_prunable(&drop, Some(boundary)).unwrap();
    assert_eq!(
        stats_b_after, stats_b,
        "a real prune of proj-a's same-id node and edges must leave proj-b's own preview unchanged"
    );
}

// ---------------------------------------------------------------------------------------
// Store::count_derived_duplicates
// ---------------------------------------------------------------------------------------

#[test]
fn count_derived_duplicates_matches_prune_derived_indexs_per_type_report_in_declared_order_zeros_included(
) {
    let backend = Store::open(":memory:").unwrap();
    let project = "proj-preview";
    let store = Namespaced::new(&backend, project);

    let mut events = Vec::with_capacity(CODE_ROUNDS + EDGE_ROUNDS);
    for r in 0..CODE_ROUNDS {
        events.push(keyed(
            rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
            code_entity(),
            KEY_CODE,
            1_000 + r as u64,
        ));
    }
    for r in 0..EDGE_ROUNDS {
        events.push(keyed(
            rigger::contextgraph::TYPE_EDGE_INFERRED,
            edge_inferred(),
            KEY_EDGE,
            2_000 + r as u64,
        ));
    }
    // DocConceptExtracted and DocLinkExtracted are left at ZERO occurrences on purpose - the report
    // must still name them, at 0, in their declared position.
    store
        .append(rigger::conductor::STREAM, ExpectedRevision::Any, &events)
        .unwrap();

    let identity = rigger::ingest::derived_index_identity();
    let prefix = Namespaced::prefix_for(project);

    let expected = vec![
        (
            rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED.to_string(),
            CODE_ROUNDS - 1,
        ),
        (
            rigger::contextgraph::TYPE_EDGE_INFERRED.to_string(),
            EDGE_ROUNDS - 1,
        ),
        (
            rigger::contextgraph::TYPE_DOC_CONCEPT_EXTRACTED.to_string(),
            0,
        ),
        (rigger::contextgraph::TYPE_DOC_LINK_EXTRACTED.to_string(), 0),
    ];

    let preview = backend
        .count_derived_duplicates(&prefix, &identity)
        .unwrap();
    assert_eq!(
        preview, expected,
        "count_derived_duplicates must report every declared type, in declared order, zeros \
         included for the two untouched types"
    );
    let preview_again = backend
        .count_derived_duplicates(&prefix, &identity)
        .unwrap();
    assert_eq!(
        preview_again, preview,
        "a read-only preview must report the same counts on repeat asks"
    );

    let pruned = backend.prune_derived_index(&prefix, &identity).unwrap();
    assert_eq!(
        pruned.removed, preview,
        "prune_derived_index must remove EXACTLY what the preview counted, per type, in the same \
         order the preview reported it in"
    );

    let after = backend
        .count_derived_duplicates(&prefix, &identity)
        .unwrap();
    assert!(
        after.iter().all(|(_, n)| *n == 0),
        "after a real prune, count_derived_duplicates must find nothing left duplicated; got {after:?}"
    );
}

#[test]
fn count_derived_duplicates_needs_no_reasserting_declaration_unlike_the_prune_it_previews() {
    let backend = Store::open(":memory:").unwrap();
    let project = "proj-undeclared";
    let store = Namespaced::new(&backend, project);
    let events = vec![
        keyed(
            rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
            code_entity(),
            KEY_CODE,
            1_000,
        ),
        keyed(
            rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
            code_entity(),
            KEY_CODE,
            1_001,
        ),
    ];
    store
        .append(rigger::conductor::STREAM, ExpectedRevision::Any, &events)
        .unwrap();

    // The SAME meta key / covered types / split as the shipped policy, with the valid-time
    // partition simply never declared.
    let shipped = rigger::ingest::derived_index_identity();
    let undeclared = ContentIdentity::new(
        shipped.meta_key().to_string(),
        shipped.types().to_vec(),
        shipped.split(),
    );
    assert!(
        undeclared.reasserting().is_none(),
        "the fixture must actually be undeclared, or this test proves nothing"
    );

    let prefix = Namespaced::prefix_for(project);

    // THE READ SUCCEEDS: a count writes nothing, so it needs no answer to "does a surviving row's
    // valid-time need to be carried forward" - the one question an undeclared partition cannot
    // answer.
    let preview = backend
        .count_derived_duplicates(&prefix, &undeclared)
        .expect("count_derived_duplicates must succeed against an undeclared partition");
    let code_dupes = preview
        .iter()
        .find(|(t, _)| t == rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED)
        .map(|(_, n)| *n);
    assert_eq!(
        code_dupes,
        Some(1),
        "the preview must still count the real duplicate; got {preview:?}"
    );

    // THE WRITE REFUSES: pruning against the SAME undeclared identity cannot know whether to carry
    // a survivor's earliest valid-time forward, and correctly refuses rather than guessing.
    let err = backend
        .prune_derived_index(&prefix, &undeclared)
        .expect_err("prune_derived_index must refuse an undeclared reasserting partition");
    assert!(
        err.to_string()
            .contains("has not declared which of its types re-assert"),
        "the refusal must name the undeclared partition as the reason; got {err}"
    );
}

#[test]
fn count_derived_duplicates_matches_its_namespace_prefix_literally_not_as_a_wildcard_pattern() {
    let backend = Store::open(":memory:").unwrap();
    // `TARGET`'s own identity carries a SQL wildcard character (`_`); `WILDCARD_NEIGHBOUR` is the
    // namespace a `LIKE`-based prefix match would sweep in with it (the `_` matching the `X`).
    const TARGET: &str = "my_repo";
    const WILDCARD_NEIGHBOUR: &str = "myXrepo";

    for project in [TARGET, WILDCARD_NEIGHBOUR] {
        let store = Namespaced::new(&backend, project);
        let events = vec![
            keyed(
                rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                code_entity(),
                KEY_CODE,
                1_000,
            ),
            keyed(
                rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                code_entity(),
                KEY_CODE,
                1_001,
            ),
        ];
        store
            .append(rigger::conductor::STREAM, ExpectedRevision::Any, &events)
            .unwrap();
    }

    let identity = rigger::ingest::derived_index_identity();
    let target_prefix = Namespaced::prefix_for(TARGET);
    let preview = backend
        .count_derived_duplicates(&target_prefix, &identity)
        .unwrap();
    let code_dupes = preview
        .iter()
        .find(|(t, _)| t == rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED)
        .map(|(_, n)| *n);
    assert_eq!(
        code_dupes,
        Some(1),
        "count_derived_duplicates on my_repo must count ONLY my_repo's own duplicate (1), not \
         myXrepo's as well - a LIKE-based prefix match would double it; got {preview:?}"
    );

    // A real prune scoped to the target must leave the wildcard-lookalike neighbour untouched.
    backend
        .prune_derived_index(&target_prefix, &identity)
        .unwrap();
    let neighbour_prefix = Namespaced::prefix_for(WILDCARD_NEIGHBOUR);
    let neighbour_after = backend
        .count_derived_duplicates(&neighbour_prefix, &identity)
        .unwrap();
    let neighbour_code = neighbour_after
        .iter()
        .find(|(t, _)| t == rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED)
        .map(|(_, n)| *n);
    assert_eq!(
        neighbour_code,
        Some(1),
        "myXrepo's own duplicate must survive untouched by a prune scoped to my_repo"
    );
}
