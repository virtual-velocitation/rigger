//! Periphery (contract / API / integration) test for spec 41 criterion 1: the prune authority the
//! context graph reclaims superseded structural edges through. Spec 41 extends `Projector::prune`
//! with a retention boundary so `rigger reset --runs` reclaims the superseded-edge accumulation
//! (`valid_to IS NOT NULL` rows retired before the active run) that grounding never reads but every
//! fold and traversal carries. Because that changes the PUBLIC prune signature and adds the public
//! `PruneStats` return, this layer guards the boundary the inside-out fold unit test is structurally
//! blind to.
//!
//! The implementer's inside-out unit test reads the private `edges` table directly to count
//! superseded rows before and after. This test runs OUTSIDE the crate, over the library's public
//! surface only - `Projector::open` -> `Projection::apply` -> the extended `prune` -> the public
//! `PruneStats` and `Projection::subgraph` - and cannot reach `conn`, exactly as an external caller
//! cannot. It pins that driving the extended prune at the active-run boundary (a) reclaims exactly
//! the superseded edges retired BEFORE it, reported on `PruneStats::superseded_edges`, and (b)
//! leaves the LIVE `subgraph` a grounding consumer reads byte-for-byte unchanged (the reclamation
//! removed only historical rows).
//!
//! Scope is strictly criterion 1 (the superseded-edge prune mechanism at the public boundary). The
//! dedicated live-invariant guarantee (criterion 2) and the bounded-growth regression (criterion 3)
//! are owned by sibling in-crate units; this file's role is to guard the changed PUBLIC prune
//! surface, not to re-derive those criteria.

use rigger::contextgraph::sqlite::{Projector, PruneStats};
use rigger::contextgraph::{Graph, Projection, REL_CONTAINS, TYPE_CODE_ENTITY_EXTRACTED};
use rigger::eventstore::Event;
use std::time::{Duration, UNIX_EPOCH};

/// Fold a `CodeEntityExtracted` (`file` defines `name`) from its raw on-log JSON at `pos`, exactly
/// the event the extraction pass emits per definition - deliberately built from JSON so the test
/// pins the on-log contract, not the in-crate payload struct. `fresh` marks the FIRST event of an
/// extraction batch, whose fold supersedes the file's prior live structural edges before folding the
/// new batch. `secs` sets the event's `valid_from` (when the extraction happened), so a re-extraction
/// batch's supersession stamps `valid_to = to_nanos(valid_from)` - the retention boundary spec 41
/// keys on. `apply` returns `Err` on a fold failure, so a successful call is itself evidence the
/// payload folded.
fn apply_def(p: &Projector, pos: u64, file: &str, name: &str, line: u32, fresh: bool, secs: u64) {
    let payload = serde_json::json!({
        "file": file, "name": name, "kind": "function", "line": line, "lang": "rust",
        "fresh": fresh,
    });
    let mut e = Event::new(
        TYPE_CODE_ENTITY_EXTRACTED,
        serde_json::to_vec(&payload).unwrap(),
    )
    .with_valid_from(UNIX_EPOCH + Duration::from_secs(secs));
    e.position = pos;
    p.apply(&e).unwrap();
}

/// The nanosecond boundary an edge carries for a fact retired `secs` after the epoch - the public
/// mirror of the crate-private `to_nanos`, computed here so the external test never reaches into the
/// crate for it. This is the same time base an edge's `valid_to` is stored in.
fn nanos(secs: u64) -> i64 {
    Duration::from_secs(secs).as_nanos() as i64
}

/// The file's live CONTAINS targets in a public `subgraph` result, sorted - the exact live structure
/// a grounding consumer reads. The prune must leave this identical (it reclaims only historical rows).
fn live_contains(g: &Graph, file: &str) -> Vec<String> {
    let mut out: Vec<String> = g
        .edges
        .iter()
        .filter(|e| e.rel == REL_CONTAINS && e.from == file)
        .map(|e| e.to.clone())
        .collect();
    out.sort();
    out
}

#[test]
fn extended_prune_reclaims_superseded_edges_at_the_boundary_leaving_the_live_subgraph_unchanged() {
    // Spec 41 criterion 1, proven at the PUBLIC boundary. A file re-extracted across three runs
    // accretes a superseded CONTAINS edge per prior extraction; the extended prune, driven only
    // through the public surface, reclaims the ones retired BEFORE the active-run boundary and
    // reports the count on `PruneStats`, while the LIVE subgraph a consumer reads is untouched.
    let p = Projector::open(":memory:", "test").unwrap();
    let file = "src/a.rs";

    // Run 1 (t=100s): first extraction of `foo` and `bar` - two live CONTAINS edges.
    apply_def(&p, 1, file, "foo", 5, true, 100);
    apply_def(&p, 2, file, "bar", 9, false, 100);
    // Run 2 (t=200s): re-extract `foo` only. The fresh event supersedes run-1's CONTAINS(foo)+(bar)
    // with valid_to=to_nanos(200s), then folds a new live CONTAINS(foo).
    apply_def(&p, 10, file, "foo", 12, true, 200);
    // Run 3 (t=300s, the ACTIVE run): re-extract `foo` and add `baz`. Supersedes run-2's CONTAINS(foo)
    // with valid_to=to_nanos(300s); folds live CONTAINS(foo)+CONTAINS(baz).
    apply_def(&p, 20, file, "foo", 3, true, 300);
    apply_def(&p, 21, file, "baz", 7, false, 300);

    let boundary = nanos(300);

    // The public LIVE view before the prune: exactly the active run's structure (foo, baz). The
    // superseded rows never surface here - `subgraph` filters `valid_to IS NULL` - so a consumer
    // already sees only LIVE, which is exactly what the prune must preserve.
    let before = p.subgraph(&[file.to_string()], 1).unwrap();
    assert_eq!(
        live_contains(&before, file),
        vec!["src/a.rs::baz".to_string(), "src/a.rs::foo".to_string()],
        "the live subgraph shows the active run's entities (foo, baz), not the removed bar"
    );

    // Drive the extended prune at the active-run boundary with NO dead-run nodes: only the spec-41
    // superseded-edge reclamation. It reclaims exactly the two rows retired before the boundary
    // (run-1 foo + bar, valid_to=200s); the run-2 row (valid_to=300s == boundary) is recent history.
    let stats: PruneStats = p.prune(&[], Some(boundary)).unwrap();
    assert_eq!(
        stats,
        PruneStats {
            nodes: 0,
            superseded_edges: 2,
        },
        "the public prune reports exactly the two superseded edges retired before the boundary, and no node drop"
    );

    // The LIVE subgraph a grounding consumer reads is byte-for-byte identical after the prune - the
    // reclamation removed only historical rows, never a live edge (LIVE is sacrosanct at the boundary).
    let after = p.subgraph(&[file.to_string()], 1).unwrap();
    assert_eq!(
        live_contains(&after, file),
        live_contains(&before, file),
        "the public live view is unchanged by the reclamation; only historical rows were reclaimed"
    );

    // Re-pruning at the same boundary now reclaims nothing: the pre-boundary rows are gone and the
    // boundary-time row is retained, so the historical tail is bounded, not re-scanned into removals.
    let again: PruneStats = p.prune(&[], Some(boundary)).unwrap();
    assert_eq!(
        again.superseded_edges, 0,
        "a second prune at the same boundary reclaims nothing - the reclamation is a stable set operation"
    );
}
