//! PERIPHERY (contract / API) tests for spec 68, criterion 3 - the READ-ONLY preview primitive
//! the bare menu's `--runs` line is built on: `contextgraph::sqlite::Projector::count_prunable`.
//!
//! `tests/reset_menu.rs` drives the compiled binary end-to-end and proves the printed menu line
//! agrees with a real flagged prune - but its fixture exercises only ONE dimension of
//! `PruneStats` (dead-run nodes), read back as a substring of the binary's stdout. This file
//! drives the `pub fn` DIRECTLY at the library's public surface, to close what a
//! subprocess/stdout comparison cannot reach precisely:
//!
//!   1. **The EDGES dimension.** No other test seeds a superseded structural edge and checks
//!      `count_prunable`'s `superseded_edges` field. This proves `count_prunable` reports the
//!      SAME edge count `prune` then actually reclaims, not only the node count, and that asking
//!      twice never consumes anything the real prune would then miss.
//!   2. **Project scoping on a shared backend.** `count_prunable`'s own doc promises it is
//!      "scoped to self.project exactly like prune - a shared backend never counts another
//!      project's same-id node or edge" - a promise no other test exercises with two projects
//!      sharing one graph.db file.

mod common;

use common::cli::nanos;
use common::fixtures::apply_def_at;
use rigger::contextgraph::sqlite::{Projector, PruneStats};

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
    apply_def_at(&p, 1, file, "foo", 5, true, 100);
    apply_def_at(&p, 2, file, "bar", 9, false, 100);
    apply_def_at(&p, 10, file, "foo", 12, true, 200);
    apply_def_at(&p, 20, file, "foo", 3, true, 300);
    apply_def_at(&p, 21, file, "baz", 7, false, 300);

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
        apply_def_at(p, base + 1, "src/a.rs", "foo", 5, true, 100);
        apply_def_at(p, base + 2, "src/a.rs", "bar", 9, false, 100);
        apply_def_at(p, base + 10, "src/a.rs", "foo", 12, true, 200);
        apply_def_at(p, base + 20, "src/a.rs", "foo", 3, true, 300);
        apply_def_at(p, base + 21, "src/a.rs", "baz", 7, false, 300);
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
