//! Periphery (contract / API / integration) tests for spec 43 criterion 5: the run-tree's
//! click-to-seed is re-pointed OFF the (now-absent) unit node and ONTO the decisions and findings a
//! unit produced and the files they concern. With the KIND_UNIT node de-noised away, a raw unit-id
//! seed would land nowhere; `dash::graph_seeds` therefore enumerates the content ids the run
//! produced plus the files those decisions GOVERN and those findings are ABOUT - the nodes that
//! remain in the graph - so clicking a unit still lands on a real, non-empty neighborhood.
//!
//! These drive the crate's PUBLIC surface as a downstream consumer does, and wire the two public
//! APIs the run-tree seam actually crosses together: `rigger::dash::graph_seeds` (compute the seeds
//! from a run's events) feeding `rigger::contextgraph`'s `Projector` / `subgraph` (resolve them to a
//! neighborhood). The inside-out unit test exercises `graph_seeds` through in-crate helpers; this
//! layer pins the observable contract at the module boundary the dash inspector consumes.

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    Projection, KIND_UNIT, TYPE_DECISION_MADE, TYPE_GATE_VERDICT, TYPE_REVIEW_FINDING,
    TYPE_UNIT_INTEGRATED, TYPE_UNIT_STARTED,
};
use rigger::dash::graph_seeds;
use rigger::eventstore::Event;

/// Build one event from its raw on-log JSON at `pos` - the shape the loop actually records.
fn ev(pos: u64, type_: &str, payload: serde_json::Value) -> Event {
    let mut e = Event::new(type_, serde_json::to_vec(&payload).unwrap());
    e.position = pos;
    e
}

/// A whole run's events: a unit, a decision it made (governing two files), a finding it drew (about
/// one file), a gate verdict, and the unit's integration - each in its raw production shape.
fn a_run() -> Vec<Event> {
    vec![
        ev(
            1,
            TYPE_UNIT_STARTED,
            serde_json::json!({ "unit": "u1", "criterion": "c", "agent": "impl", "needs": ["u0"] }),
        ),
        ev(
            2,
            TYPE_DECISION_MADE,
            serde_json::json!({ "id": "d1", "summary": "use the shared authority", "governs": ["a.rs", "b.rs"], "supersedes": "" }),
        ),
        ev(
            3,
            TYPE_REVIEW_FINDING,
            serde_json::json!({ "id": "f1", "by": "sdet", "unit": "u1", "summary": "y", "about": ["c.rs"] }),
        ),
        ev(
            4,
            TYPE_GATE_VERDICT,
            serde_json::json!({ "gate": "fmt", "pass": true }),
        ),
        ev(
            5,
            TYPE_UNIT_INTEGRATED,
            serde_json::json!({ "id": "u1", "commit": "abc1234" }),
        ),
    ]
}

#[test]
fn graph_seeds_re_point_off_units_onto_decisions_findings_and_their_files() {
    // The re-pointed contract: the seed set is exactly the decision and finding ids the run produced
    // plus the files they GOVERN / are ABOUT - deterministically ordered (a sorted set) - and NEVER a
    // unit id (its own, or a dependency's) or a gate id, because those are no longer graph nodes.
    let seeds = graph_seeds(&a_run());
    assert_eq!(
        seeds,
        vec![
            "a.rs".to_string(),
            "b.rs".to_string(),
            "c.rs".to_string(),
            "d1".to_string(),
            "f1".to_string(),
        ],
        "seeds are the decisions + findings + the files they concern, in deterministic sorted order"
    );
    assert!(
        !seeds.contains(&"u1".to_string()),
        "the unit's own id is never a seed (it is not a graph node)"
    );
    assert!(
        !seeds.contains(&"u0".to_string()),
        "a dependency unit id is never a seed"
    );
    assert!(
        !seeds.contains(&"fmt".to_string()),
        "a gate id is never a seed"
    );
}

#[test]
fn a_runs_seeds_land_on_a_real_neighborhood_through_the_public_projection() {
    // End-to-end across the run-tree seam a consumer crosses: fold the run into a real projection,
    // then seed that projection with `graph_seeds`' output and read the neighborhood. The re-pointed
    // seeds must resolve to a NON-EMPTY neighborhood carrying the unit's decision, its finding, and
    // the files they produced - and NO KIND_UNIT node exists, proving the click landed via the
    // content/code the unit produced, not via a (gone) unit node.
    let run = a_run();
    let p = Projector::open(":memory:", "test").unwrap();
    for e in &run {
        p.apply(e).unwrap();
    }

    let seeds = graph_seeds(&run);
    let g = p.subgraph(&seeds, 2).unwrap();

    assert!(
        !g.nodes.is_empty(),
        "the re-pointed unit seed lands on a real, non-empty neighborhood, not an empty result"
    );
    assert!(
        g.nodes.iter().any(|n| n.id == "d1"),
        "the unit's decision is in the seeded neighborhood"
    );
    assert!(
        g.nodes.iter().any(|n| n.id == "f1"),
        "the unit's finding is in the seeded neighborhood"
    );
    assert!(
        g.nodes.iter().any(|n| n.id == "a.rs"),
        "a file the unit's decision governs is in the seeded neighborhood"
    );
    assert!(
        g.nodes.iter().any(|n| n.id == "c.rs"),
        "the file the unit's finding is about is in the seeded neighborhood"
    );
    assert!(
        !g.nodes.iter().any(|n| n.kind == KIND_UNIT),
        "no KIND_UNIT node exists (the machinery is gone); the seed landed via the decisions/files"
    );
    assert!(
        !g.nodes.iter().any(|n| n.id == "u1"),
        "the unit id is not a node in the neighborhood"
    );
}
