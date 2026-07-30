//! Periphery (contract / API / integration) tests for spec 40 criterion 3: the graph is a
//! REBUILDABLE projection of the log, so the operational cleanup for the measured pile of duplicate
//! live edges is a fresh graph REBUILD - and that rebuild is a pure, reproducible function of the
//! log. These run OUTSIDE the crate, over the library's public surface, so they guard the boundary
//! the inside-out fold unit test is structurally blind to.
//!
//! The dedup is demonstrated over the surviving `GOVERNS` (decision -> file) content edge: the spec
//! 43 de-noise dropped the old `agent --TOUCHES--> file` machinery edge the fold once projected, but
//! `add_edge`'s collapse behaviour is edge-agnostic, so a re-asserted decision->file GOVERNS edge
//! rebuilds exactly as a re-touch once did.
//!
//! The implementer's inside-out unit test seeds the dirty on-disk pile with a raw
//! `INSERT ... valid_to = NULL` through the private `Projector::conn` and reads the private `edges`
//! table directly, so it proves the ROW COUNT the rebuild collapses. This layer instead drives the
//! PUBLIC projection a grounding consumer actually reads - `Projector::open` -> `Projection::apply`
//! -> `Projection::subgraph` - and cannot reach `conn`, exactly as an external caller cannot. It
//! pins that a fresh rebuild of a log that re-asserts one relationship N times surfaces exactly ONE
//! live `GOVERNS` edge per `(from, rel, to)` in the public `Graph` (the collapse a consumer sees),
//! and that TWO independent fresh rebuilds of the SAME log re-derive the IDENTICAL public subgraph.
//!
//! Scope is strictly criterion 3 (rebuild-dedup / projection idempotency). The single-fold collapse
//! of a re-asserted relationship (criterion 1) and the live-only scoping after an invalidation
//! (criterion 2) are owned by sibling units; this file's distinctive guard is the REPRODUCIBILITY of
//! the rebuild across independent fresh folds, which the single-fold criterion-1 periphery test does
//! not exercise. It leans on (but does not own) the upsert-live `add_edge` fold arm that landed in
//! criterion 1.

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{Edge, Graph, Projection, REL_GOVERNS, TYPE_DECISION_MADE};
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

/// A REBUILD: fold the canonical criterion-3 log from scratch into a FRESH, empty projection and
/// return the public `subgraph` over both governed files. Each call is an independent rebuild - a new
/// in-memory db, no shared state - so folding the SAME log twice must yield equal results if (and
/// only if) the projection is a pure function of the log.
///
/// The log re-asserts `d1 --GOVERNS--> src/f.rs` 45 times (positions 1..=45; valid_from
/// 100..=4500s) - the worst-case duplicate pile the old bare-insert fold would have accreted as 45
/// live rows - then folds two DISTINCT relationships (a different decision, a different file) that
/// must each survive the rebuild as their own single live edge.
fn rebuild() -> Graph {
    let p = Projector::open(":memory:", "test").unwrap();
    for pos in 1..=45u64 {
        apply_governs(&p, pos, "d1", "src/f.rs", 100 * pos);
    }
    apply_governs(&p, 46, "d2", "src/f.rs", 5000);
    apply_governs(&p, 47, "d1", "src/g.rs", 6000);
    // Seed BOTH files so the reachable set is {src/f.rs, src/g.rs, d1, d2} and every edge above has
    // both endpoints in scope - the one query surfaces all three distinct live edges.
    p.subgraph(&["src/f.rs".to_string(), "src/g.rs".to_string()], 1)
        .unwrap()
}

#[test]
fn a_rebuild_folds_the_log_into_one_live_edge_per_relationship_through_the_public_subgraph() {
    // Spec 40 criterion 3, proven at the PUBLIC boundary. The graph is a rebuildable projection, so
    // the operational cleanup for the 45-strong duplicate pile is a fresh fold from scratch. The
    // public `subgraph` edge fetch is not `SELECT DISTINCT`, so under the old bare-insert fold it
    // would surface 45 `GOVERNS` rows for the one relationship. The upsert-live `add_edge` collapses
    // every re-assert into the ONE live edge, so the rebuilt projection a consumer reads carries
    // exactly ONE edge per `(from, rel, to)` - bumped to the LATEST assertion's `source` (45) and
    // keeping the EARLIEST `valid_from` (100s) - while a DIFFERENT decision or a DIFFERENT file each
    // survives the rebuild as its own distinct live edge.
    let g = rebuild();
    assert_eq!(
        governs(&g.edges),
        vec![
            // d1->f: 45 duplicate live edges collapsed to ONE; source = latest (45), valid_from = earliest (100s).
            ("d1".to_string(), "src/f.rs".to_string(), 45, nanos(100)),
            // a different FILE is a distinct relationship, its own single live edge.
            ("d1".to_string(), "src/g.rs".to_string(), 47, nanos(6000)),
            // a different DECISION is a distinct relationship, its own single live edge.
            ("d2".to_string(), "src/f.rs".to_string(), 46, nanos(5000)),
        ],
        "a fresh rebuild must surface exactly ONE live GOVERNS edge per (from,rel,to) with latest \
         source + earliest valid_from; distinct relationships each survive as their own single edge"
    );
}

#[test]
fn two_independent_rebuilds_of_the_same_log_re_derive_the_identical_public_subgraph() {
    // The criterion-3-DISTINCTIVE guard: a rebuild is a PURE, reproducible function of the log. Fold
    // the SAME log into two INDEPENDENT fresh projections (separate in-memory dbs, no shared state)
    // and read each one's PUBLIC subgraph: the two must be byte-for-byte identical. This reddens if
    // a rebuild ever depended on pre-existing on-disk state, arrival timing, or any hidden mutable
    // state instead of the log alone - the property that makes "just rebuild the graph" a sound
    // operational cleanup for the accreted duplicates.
    let first = governs(&rebuild().edges);
    let second = governs(&rebuild().edges);

    assert_eq!(
        first, second,
        "rebuilding the same log from scratch must re-derive the identical public subgraph"
    );
    // Pin the shared value too, so the equality above is anchored to the ACTUAL deduped set (three
    // single live edges), never satisfied by two identically-wrong rebuilds.
    assert_eq!(
        first,
        vec![
            ("d1".to_string(), "src/f.rs".to_string(), 45, nanos(100)),
            ("d1".to_string(), "src/g.rs".to_string(), 47, nanos(6000)),
            ("d2".to_string(), "src/f.rs".to_string(), 46, nanos(5000)),
        ],
        "each independent rebuild collapses the 45 duplicates to one live edge per relationship"
    );
}
