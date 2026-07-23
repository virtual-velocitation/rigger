//! Periphery (contract / API / integration) tests for spec 40 criterion 1: the upsert-live fold
//! collapses a re-asserted `TOUCHES` relationship to at most ONE live edge. These run OUTSIDE the
//! crate, over the library's public surface, so they guard the boundary the inside-out fold unit
//! test is structurally blind to.
//!
//! The implementer's inside-out unit test reads the private `edges` TABLE directly (its own
//! `p.conn.lock()` + a raw `SELECT ... FROM edges`), so it proves the ROW COUNT in the table. This
//! layer instead drives the PUBLIC projection the grounding slice injected into every prompt is
//! actually built from: `Projector::open` -> `Projection::apply` -> `Projection::subgraph`. The
//! `subgraph` edge fetch is its OWN SQL (`SELECT from_id, to_id, ... WHERE valid_to IS NULL AND
//! from_id IN (...) AND to_id IN (...)`) with NO `SELECT DISTINCT`, so under the old bare-insert
//! fold it returned one row per accreted duplicate. These tests pin that the public `Graph` a
//! consumer sees collapses N re-asserts to ONE live `TOUCHES` edge carrying the LATEST assertion's
//! `source` and the EARLIEST `valid_from`, while a DIFFERENT agent or a DIFFERENT file still folds
//! its own distinct live edge (dedup removes only EXACT `(from, rel, to, tier)` duplicates).
//!
//! Scope is strictly criterion 1 (the `TOUCHES` re-assert fold). The live-only scoping after an
//! invalidation (criterion 2) and the rebuild-collapse of pre-existing duplicates (criterion 3) are
//! owned by sibling units and are not exercised here.

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{Edge, Projection, REL_TOUCHES, TYPE_FILE_TOUCHED};
use rigger::eventstore::Event;
use std::time::{Duration, UNIX_EPOCH};

/// Fold a `FileTouched` (`by` touches `path`) built from its raw on-log JSON at `pos` - exactly the
/// event the loop records each time an agent writes a file - deliberately bypassing the in-crate
/// payload struct so the test pins the JSON contract, not the Rust type. `secs` sets the event's
/// valid-from (when the touch happened) so a test can assert the collapsed edge keeps the EARLIEST
/// assertion time; `pos` becomes the edge's `source`, so the LATEST assertion wins. `apply` returns
/// `Err` on a fold failure, so a successful call is itself evidence the payload folded.
fn apply_touch(p: &Projector, pos: u64, by: &str, path: &str, secs: u64) {
    let payload = serde_json::json!({ "path": path, "by": by });
    let mut e = Event::new(TYPE_FILE_TOUCHED, serde_json::to_vec(&payload).unwrap())
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

/// Every live `TOUCHES` edge in a public `subgraph` result as `(from, to, source, valid_from)`,
/// sorted, so a test can COUNT the rows the public projection exposes and read their provenance.
fn touches(graph_edges: &[Edge]) -> Vec<(String, String, u64, i64)> {
    let mut out: Vec<_> = graph_edges
        .iter()
        .filter(|e| e.rel == REL_TOUCHES)
        .map(|e| (e.from.clone(), e.to.clone(), e.source, e.valid_from))
        .collect();
    out.sort();
    out
}

#[test]
fn subgraph_collapses_repeated_touches_to_one_live_edge_keeping_latest_provenance() {
    // Spec 40 criterion 1, proven at the PUBLIC boundary. Every `FileTouched` re-asserts
    // `agent --TOUCHES--> file`; the old bare-insert fold appended a fresh live row per touch, so
    // the public `subgraph` (its edge fetch is not DISTINCT) would surface N `TOUCHES` edges for one
    // relationship. The upsert-live fold collapses the re-assert into the ONE existing live edge, so
    // the projection a grounding consumer reads carries exactly ONE edge per `(from, rel, to, tier)`
    // - bumped to the LATEST assertion's `source` and keeping the EARLIEST `valid_from` - while a
    // DIFFERENT agent or a DIFFERENT file keeps its own distinct live edge.
    let p = Projector::open(":memory:", "test").unwrap();

    // agent-a touches src/f.rs four times (positions 10..=13; valid_from 100..=400s).
    apply_touch(&p, 10, "agent-a", "src/f.rs", 100);
    apply_touch(&p, 11, "agent-a", "src/f.rs", 200);
    apply_touch(&p, 12, "agent-a", "src/f.rs", 300);
    apply_touch(&p, 13, "agent-a", "src/f.rs", 400);
    // A DIFFERENT agent and a DIFFERENT file each fold their own distinct live edge.
    apply_touch(&p, 14, "agent-b", "src/f.rs", 500);
    apply_touch(&p, 15, "agent-a", "src/g.rs", 600);

    // Seed BOTH files so the reachable set is {src/f.rs, src/g.rs, agent-a, agent-b} and every edge
    // above has both endpoints in scope - the one query surfaces all three distinct live edges.
    let g = p
        .subgraph(&["src/f.rs".to_string(), "src/g.rs".to_string()], 1)
        .unwrap();

    assert_eq!(
        touches(&g.edges),
        vec![
            // a->f: FOUR folds collapsed to ONE live edge; source = latest (13), valid_from = earliest (100s).
            ("agent-a".to_string(), "src/f.rs".to_string(), 13, nanos(100)),
            // a different FILE is a distinct edge, untouched by the a->f dedup.
            ("agent-a".to_string(), "src/g.rs".to_string(), 15, nanos(600)),
            // a different AGENT is a distinct edge, untouched by the a->f dedup.
            ("agent-b".to_string(), "src/f.rs".to_string(), 14, nanos(500)),
        ],
        "public subgraph must surface ONE live TOUCHES edge per (from,rel,to) with latest source + \
         earliest valid_from; a different agent/file stays a distinct edge"
    );
}

#[test]
fn the_collapsed_edge_keeps_the_earliest_fact_time_and_latest_source_regardless_of_arrival_order() {
    // The dedup UPDATE keeps `min(valid_from)` and `max(source)`, so the collapsed provenance is
    // order-INDEPENDENT: an event's valid_from is the caller-supplied "when the fact became true",
    // which need not arrive in log-position order. Fold three touches whose valid_from is
    // NON-MONOTONIC in position (pos 20/21/22 -> secs 300/100/200): the surviving edge must carry
    // valid_from = the EARLIEST fact time (100s, which arrived in the MIDDLE at pos 21) and
    // source = the LATEST position (22). This reddens if the fold took last-write / first-write for
    // either field instead of a true min/max.
    let p = Projector::open(":memory:", "test").unwrap();
    apply_touch(&p, 20, "agent-a", "src/f.rs", 300);
    apply_touch(&p, 21, "agent-a", "src/f.rs", 100);
    apply_touch(&p, 22, "agent-a", "src/f.rs", 200);

    let g = p.subgraph(&["src/f.rs".to_string()], 1).unwrap();

    assert_eq!(
        touches(&g.edges),
        vec![(
            "agent-a".to_string(),
            "src/f.rs".to_string(),
            22,
            nanos(100)
        )],
        "collapsed edge keeps earliest valid_from (100s) and latest source (22) independent of \
         arrival order"
    );
}
