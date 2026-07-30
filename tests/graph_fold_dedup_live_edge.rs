//! Periphery (contract / API / integration) tests for spec 40 criterion 1: the upsert-live fold
//! collapses a re-asserted relationship to at most ONE live edge. These run OUTSIDE the crate, over
//! the library's public surface, so they guard the boundary the inside-out fold unit test is
//! structurally blind to.
//!
//! The dedup is demonstrated over the surviving `GOVERNS` (decision -> file) content edge: the spec
//! 43 de-noise dropped the old `agent --TOUCHES--> file` machinery edge the fold once projected, but
//! `add_edge`'s collapse-a-re-assertion-into-the-one-live-edge behaviour is edge-agnostic, so a
//! re-asserted decision->file GOVERNS edge exercises it exactly as a re-touch once did.
//!
//! The implementer's inside-out unit test reads the private `edges` TABLE directly (its own
//! `p.conn.lock()` + a raw `SELECT ... FROM edges`), so it proves the ROW COUNT in the table. This
//! layer instead drives the PUBLIC projection the grounding slice injected into every prompt is
//! actually built from: `Projector::open` -> `Projection::apply` -> `Projection::subgraph`. The
//! `subgraph` edge fetch is its OWN SQL (`SELECT from_id, to_id, ... WHERE valid_to IS NULL AND
//! from_id IN (...) AND to_id IN (...)`) with NO `SELECT DISTINCT`, so under the old bare-insert
//! fold it returned one row per accreted duplicate. These tests pin that the public `Graph` a
//! consumer sees collapses N re-asserts to ONE live `GOVERNS` edge carrying the LATEST assertion's
//! `source` and the EARLIEST `valid_from`, while a DIFFERENT decision or a DIFFERENT file still folds
//! its own distinct live edge (dedup removes only EXACT `(from, rel, to, tier)` duplicates).
//!
//! Scope is strictly criterion 1 (the re-assert fold). The live-only scoping after an invalidation
//! (criterion 2) and the rebuild-collapse of pre-existing duplicates (criterion 3) are owned by
//! sibling units and are not exercised here.

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{Edge, Projection, REL_GOVERNS, TYPE_DECISION_MADE};
use rigger::eventstore::Event;
use std::time::{Duration, UNIX_EPOCH};

/// Fold a `DecisionMade` (`id` GOVERNS `path`) built from its raw on-log JSON at `pos` - deliberately
/// bypassing the in-crate payload struct so the test pins the JSON contract, not the Rust type.
/// GOVERNS is the surviving content edge the dedup is demonstrated over after the spec 43 de-noise
/// dropped the old TOUCHES machinery vehicle. `secs` sets the event's valid-from so a test can assert
/// the collapsed edge keeps the EARLIEST assertion time; `pos` becomes the edge's `source`, so the
/// LATEST assertion wins. `apply` returns `Err` on a fold failure, so a successful call is itself
/// evidence the payload folded.
fn apply_governs(p: &Projector, pos: u64, id: &str, path: &str, secs: u64) {
    let payload = serde_json::json!({
        "id": id, "summary": "x", "governs": [path], "supersedes": "",
    });
    let mut e = Event::new(TYPE_DECISION_MADE, serde_json::to_vec(&payload).unwrap())
        .with_valid_from(UNIX_EPOCH + Duration::from_secs(secs));
    e.position = pos;
    p.apply(&e).unwrap();
}

/// The nanosecond `valid_from` an edge carries for a fact that became true `secs` after the epoch -
/// the public mirror of the crate-private `to_nanos`, computed here so the external test never
/// reaches into the crate for it.
fn nanos(secs: u64) -> i64 {
    Duration::from_secs(secs).as_nanos() as i64
}

/// Every live `GOVERNS` edge in a public `subgraph` result as `(from, to, source, valid_from)`,
/// sorted, so a test can COUNT the rows the public projection exposes and read their provenance.
fn governs(graph_edges: &[Edge]) -> Vec<(String, String, u64, i64)> {
    let mut out: Vec<_> = graph_edges
        .iter()
        .filter(|e| e.rel == REL_GOVERNS)
        .map(|e| (e.from.clone(), e.to.clone(), e.source, e.valid_from))
        .collect();
    out.sort();
    out
}

#[test]
fn subgraph_collapses_repeated_governs_to_one_live_edge_keeping_latest_provenance() {
    // Spec 40 criterion 1, proven at the PUBLIC boundary. Every re-fold of a decision that GOVERNS a
    // file re-asserts `decision --GOVERNS--> file`; the old bare-insert fold appended a fresh live row
    // per assertion, so the public `subgraph` (its edge fetch is not DISTINCT) would surface N
    // `GOVERNS` edges for one relationship. The upsert-live fold collapses the re-assert into the ONE
    // existing live edge, so the projection a grounding consumer reads carries exactly ONE edge per
    // `(from, rel, to, tier)` - bumped to the LATEST assertion's `source` and keeping the EARLIEST
    // `valid_from` - while a DIFFERENT decision or a DIFFERENT file keeps its own distinct live edge.
    let p = Projector::open(":memory:", "test").unwrap();

    // d1 governs src/f.rs four times (positions 10..=13; valid_from 100..=400s).
    apply_governs(&p, 10, "d1", "src/f.rs", 100);
    apply_governs(&p, 11, "d1", "src/f.rs", 200);
    apply_governs(&p, 12, "d1", "src/f.rs", 300);
    apply_governs(&p, 13, "d1", "src/f.rs", 400);
    // A DIFFERENT decision and a DIFFERENT file each fold their own distinct live edge.
    apply_governs(&p, 14, "d2", "src/f.rs", 500);
    apply_governs(&p, 15, "d1", "src/g.rs", 600);

    // Seed BOTH files so the reachable set is {src/f.rs, src/g.rs, d1, d2} and every edge above has
    // both endpoints in scope - the one query surfaces all three distinct live edges.
    let g = p
        .subgraph(&["src/f.rs".to_string(), "src/g.rs".to_string()], 1)
        .unwrap();

    assert_eq!(
        governs(&g.edges),
        vec![
            // d1->f: FOUR folds collapsed to ONE live edge; source = latest (13), valid_from = earliest (100s).
            ("d1".to_string(), "src/f.rs".to_string(), 13, nanos(100)),
            // a different FILE is a distinct edge, untouched by the d1->f dedup.
            ("d1".to_string(), "src/g.rs".to_string(), 15, nanos(600)),
            // a different DECISION is a distinct edge, untouched by the d1->f dedup.
            ("d2".to_string(), "src/f.rs".to_string(), 14, nanos(500)),
        ],
        "public subgraph must surface ONE live GOVERNS edge per (from,rel,to) with latest source + \
         earliest valid_from; a different decision/file stays a distinct edge"
    );
}

#[test]
fn the_collapsed_edge_keeps_the_earliest_fact_time_and_latest_source_regardless_of_arrival_order() {
    // The dedup UPDATE keeps `min(valid_from)` and `max(source)`, so the collapsed provenance is
    // order-INDEPENDENT: an event's valid_from is the caller-supplied "when the fact became true",
    // which need not arrive in log-position order. Fold three re-asserts whose valid_from is
    // NON-MONOTONIC in position (pos 20/21/22 -> secs 300/100/200): the surviving edge must carry
    // valid_from = the EARLIEST fact time (100s, which arrived in the MIDDLE at pos 21) and
    // source = the LATEST position (22). This reddens if the fold took last-write / first-write for
    // either field instead of a true min/max.
    let p = Projector::open(":memory:", "test").unwrap();
    apply_governs(&p, 20, "d1", "src/f.rs", 300);
    apply_governs(&p, 21, "d1", "src/f.rs", 100);
    apply_governs(&p, 22, "d1", "src/f.rs", 200);

    let g = p.subgraph(&["src/f.rs".to_string()], 1).unwrap();

    assert_eq!(
        governs(&g.edges),
        vec![("d1".to_string(), "src/f.rs".to_string(), 22, nanos(100))],
        "collapsed edge keeps earliest valid_from (100s) and latest source (22) independent of \
         arrival order"
    );
}
