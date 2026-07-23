//! Periphery (contract / API / integration) tests for spec 40 criterion 2: LIVE-ONLY scoping of the
//! upsert-live fold. `add_edge`'s dedup UPDATE carries `AND valid_to IS NULL`, so it collapses a
//! re-assertion ONLY into an edge that is currently LIVE - never into one that has since been
//! INVALIDATED. These run OUTSIDE the crate, over the library's public surface, so they guard the
//! consumer-visible facet of the guarantee that the inside-out fold unit test - which counts dead +
//! live rows in the private `edges` table - is structurally blind to.
//!
//! The inside-out test proves ROW retention: after invalidate -> re-assert, exactly one HISTORICAL
//! (dead) + one LIVE row survive in the table. That dead/live split is invisible through the public
//! API - `subgraph`'s edge fetch filters `valid_to IS NULL`, so a consumer never sees the dead row.
//! What a consumer DOES see is the projection this layer drives: `Projector::open` ->
//! `Projection::apply` -> `Projection::subgraph`. So these tests pin the PUBLICLY OBSERVABLE
//! contract of live-only scoping: a `GOVERNS` relationship that is invalidated by supersession and
//! then re-asserted is LIVE AGAIN in the projection a grounding consumer reads - the re-assertion is
//! not permanently swallowed by the (now dead) prior edge - and at most ONE live edge for the
//! relationship exists at every point in that lifecycle.
//!
//! The re-birth assertion is load-bearing on the `AND valid_to IS NULL` clause: were the dedup keyed
//! on ALL edges instead of live-only, the re-assert UPDATE would match the invalidated row (bumping
//! only `source`/`valid_from`, never clearing `valid_to`), the row would stay dead, and the public
//! `subgraph` would never surface the relationship again - the governance would be silently lost.
//!
//! Scope is strictly criterion 2 (live-only scoping after an invalidation). The `TOUCHES` re-assert
//! fold (criterion 1) lives in `graph_fold_dedup_live_edge.rs`; the rebuild-collapse of pre-existing
//! duplicates (criterion 3) is owned by a sibling unit and is not exercised here.

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{Edge, Projection, REL_GOVERNS, TYPE_DECISION_MADE};
use rigger::eventstore::Event;

/// Fold a `DecisionMade` built from its raw on-log JSON at `pos` - exactly the event the loop
/// records when an agent emits a decision - deliberately bypassing the in-crate payload struct so
/// the test pins the JSON contract, not the Rust type. Each entry in `governs` folds a
/// `decision --GOVERNS--> file` edge; a non-empty `supersedes` folds `decision --SUPERSEDES--> <id>`
/// and INVALIDATES (stamps `valid_to` on, never deletes) the superseded decision's live `GOVERNS`
/// edges. `apply` returns `Err` on a fold failure, so a successful call is itself evidence the
/// payload folded.
fn apply_decision(p: &Projector, pos: u64, id: &str, governs: &[&str], supersedes: &str) {
    let payload = serde_json::json!({
        "id": id, "summary": "", "governs": governs, "supersedes": supersedes,
    });
    let mut e = Event::new(TYPE_DECISION_MADE, serde_json::to_vec(&payload).unwrap());
    e.position = pos;
    p.apply(&e).unwrap();
}

/// The `(from, to)` of every LIVE `GOVERNS` edge a public `subgraph` result exposes, sorted, so a
/// test can COUNT the governance edges the projection actually surfaces to a grounding consumer.
fn governs_edges(graph_edges: &[Edge]) -> Vec<(String, String)> {
    let mut out: Vec<_> = graph_edges
        .iter()
        .filter(|e| e.rel == REL_GOVERNS)
        .map(|e| (e.from.clone(), e.to.clone()))
        .collect();
    out.sort();
    out
}

#[test]
fn subgraph_shows_a_superseded_governs_edge_live_again_after_re_assertion() {
    // Spec 40 criterion 2, proven at the PUBLIC boundary. Lifecycle of one `d1 --GOVERNS--> mod.rs`
    // relationship as a grounding consumer sees it through `subgraph`:
    //   assert    -> live       (folded)
    //   supersede -> NOT live    (d2 supersedes d1; d1's GOVERNS edge is invalidated, valid_to set)
    //   re-assert -> live AGAIN  (dedup keys on LIVE edges only, so it folds a FRESH live edge
    //                             rather than matching the dead row)
    // At every point at most ONE live d1->mod.rs GOVERNS edge exists. The dead row the inside-out
    // test counts is invisible here - `subgraph` filters `valid_to IS NULL` - which is exactly why
    // the consumer-visible re-birth is the facet this layer must guard.
    let p = Projector::open(":memory:", "test").unwrap();

    // 1) d1 governs mod.rs -> the projection surfaces exactly one live d1->mod.rs GOVERNS edge.
    apply_decision(&p, 1, "d1", &["mod.rs"], "");
    let after_assert = p.subgraph(&["mod.rs".to_string()], 2).unwrap();
    assert_eq!(
        governs_edges(&after_assert.edges),
        vec![("d1".to_string(), "mod.rs".to_string())],
        "after the initial assertion the consumer sees d1 governing mod.rs (one live edge)"
    );

    // 2) d2 supersedes d1 -> d1's GOVERNS edge is INVALIDATED, so the projection a consumer reads no
    //    longer surfaces d1 governing mod.rs (the invalidated edge is filtered out of the live view).
    apply_decision(&p, 2, "d2", &[], "d1");
    let after_supersede = p.subgraph(&["mod.rs".to_string()], 2).unwrap();
    assert!(
        !governs_edges(&after_supersede.edges)
            .iter()
            .any(|(from, _)| from == "d1"),
        "after supersession the invalidated d1->mod.rs GOVERNS edge is absent from the live \
         projection; got {:?}",
        governs_edges(&after_supersede.edges)
    );

    // 3) d1 is re-asserted -> because the prior d1->mod.rs GOVERNS edge is now dead, the live-only
    //    dedup matches no live row and folds a FRESH live edge. The consumer sees d1 governing mod.rs
    //    again, and exactly ONCE (the re-assertion is neither swallowed into the dead row nor
    //    duplicated). This reddens if the dedup dropped `AND valid_to IS NULL`: the re-assert would
    //    update the dead row without clearing valid_to and the relationship would stay invisible.
    apply_decision(&p, 3, "d1", &["mod.rs"], "");
    let after_reassert = p.subgraph(&["mod.rs".to_string()], 2).unwrap();
    assert_eq!(
        governs_edges(&after_reassert.edges),
        vec![("d1".to_string(), "mod.rs".to_string())],
        "the re-asserted d1->mod.rs GOVERNS edge is LIVE again in the projection and appears exactly \
         once (a fresh live edge, not swallowed into the dead row)"
    );
}

#[test]
fn subgraph_collapses_a_governs_re_assert_with_no_intervening_supersession_to_one_live_edge() {
    // The contrast that pins live-only scoping precisely: WITHOUT an intervening invalidation, a
    // re-assert of the SAME `d1 --GOVERNS--> mod.rs` relationship collapses into the ONE existing
    // live edge (the dedup UPDATE matches it), so the projection still surfaces exactly one live
    // edge. Paired with the lifecycle test above this brackets the guarantee from both sides:
    //   - drop `AND valid_to IS NULL`     -> the lifecycle test reddens (re-assert swallowed, lost).
    //   - drop the GOVERNS dedup entirely -> THIS test reddens (the plain repeat duplicates the edge).
    // So the fold dedups live edges, and ONLY live edges.
    let p = Projector::open(":memory:", "test").unwrap();

    apply_decision(&p, 1, "d1", &["mod.rs"], "");
    apply_decision(&p, 2, "d1", &["mod.rs"], ""); // re-assert, no supersession between

    let g = p.subgraph(&["mod.rs".to_string()], 2).unwrap();
    assert_eq!(
        governs_edges(&g.edges),
        vec![("d1".to_string(), "mod.rs".to_string())],
        "a plain GOVERNS re-assert collapses into the one existing live edge - the projection \
         surfaces it exactly once, never a second accreted live row"
    );
}
