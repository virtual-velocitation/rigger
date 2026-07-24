//! Periphery (contract / API / integration) tests for spec 43: the fold de-noises to the TARGET
//! PROJECT - it stops projecting the loop's own run machinery (the agent / unit / gate NODES, the
//! agent-touched-file edge, and the agent-attribution edges) while preserving every code, decision,
//! and finding node and edge a graph consumer relies on.
//!
//! These run OUTSIDE the crate, over the library's public surface - `Projector::open` ->
//! `Projection::apply` -> `Projection::subgraph` - so they pin the projection a real consumer (the
//! dash inspector, grounding, blast-radius) actually reads. The inside-out unit tests exercise each
//! fold arm in isolation through the crate-internal projector; this layer proves the arms COMPOSE:
//! folding a whole run's mixed event stream leaks NO machinery from any arm, and the content the
//! graph is about survives intact, at the exact public boundary a downstream consumer crosses.
//!
//! Two facets the per-arm unit tests do not co-locate are load-bearing here and guarded below:
//!   * the events fold from their raw PRODUCTION JSON, carrying the machinery keys the log still
//!     records (`FileTouched.by`, `UnitStarted.agent`/`needs`, `GateVerdict.pass`/`artifact`,
//!     `UnitIntegrated.commit`) - so this also pins that the de-noised fold gracefully IGNORES the
//!     now-unmodeled fields rather than erroring on them; and
//!   * the decision and finding carry a non-empty actor in event metadata (`META_ACTOR`), exactly
//!     as the conductor stamps every real emit - the precise input the deleted
//!     `e.meta.get(META_ACTOR)` code once turned into a `KIND_AGENT` node and a `DECIDED`/`RAISED`
//!     edge. If that attribution ever creeps back, the whole-run and the focused test both redden.

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    Projection, KIND_AGENT, KIND_DECISION, KIND_FINDING, KIND_GATE, KIND_UNIT, META_ACTOR,
    REL_ABOUT, REL_ASSIGNED_TO, REL_BLOCKS, REL_DECIDED, REL_GATED_BY, REL_GOVERNS, REL_RAISED,
    REL_TOUCHES, TYPE_DECISION_MADE, TYPE_FILE_TOUCHED, TYPE_GATE_VERDICT, TYPE_REVIEW_FINDING,
    TYPE_UNIT_INTEGRATED, TYPE_UNIT_STARTED,
};
use rigger::eventstore::Event;

/// Fold one event from its raw on-log JSON at `pos`, optionally stamping the acting persona in
/// `META_ACTOR` (the metadata the conductor puts on every real emit). Bypassing the in-crate
/// payload structs pins the JSON contract the log actually carries, not the Rust type. `apply`
/// returns `Err` on a fold failure, so a successful call is itself evidence the payload folded.
fn fold(p: &Projector, pos: u64, type_: &str, payload: serde_json::Value, actor: Option<&str>) {
    let mut e = Event::new(type_, serde_json::to_vec(&payload).unwrap());
    e.position = pos;
    if let Some(a) = actor {
        e.meta.insert(META_ACTOR.to_string(), a.to_string());
    }
    p.apply(&e).unwrap();
}

#[test]
fn a_whole_runs_fold_projects_the_content_but_none_of_the_machinery() {
    // Fold a whole run's mixed event stream - one of every arm the de-noise touches - into a single
    // projection, each event in its raw production shape (with the machinery keys the log still
    // records), then read the neighborhood a consumer sees through the public `subgraph`. The graph
    // must model the target project (the decision, the finding, and the code they concern) and NONE
    // of the run bookkeeping (the agent / unit / gate nodes and the machinery edges).
    let p = Projector::open(":memory:", "test").unwrap();

    // A file an agent touched - machinery: `agent --TOUCHES--> file`. De-noised to a graph no-op.
    fold(
        &p,
        1,
        TYPE_FILE_TOUCHED,
        serde_json::json!({ "path": "src/combat.rs", "by": "rust-engineer" }),
        None,
    );
    // A unit started, assigned to its agent, blocked on a dependency - machinery: the KIND_UNIT
    // node, ASSIGNED_TO, and BLOCKS. De-noised to a graph no-op.
    fold(
        &p,
        2,
        TYPE_UNIT_STARTED,
        serde_json::json!({ "unit": "u1", "criterion": "c1", "agent": "rust-engineer", "needs": ["u0"] }),
        None,
    );
    // A gate ran over a file - machinery: the KIND_GATE node and the GATED_BY edge. De-noised to a
    // graph no-op.
    fold(
        &p,
        3,
        TYPE_GATE_VERDICT,
        serde_json::json!({ "gate": "fmt", "pass": true, "artifact": "src/combat.rs" }),
        None,
    );
    // A decision governing the file - CONTENT (survives) stamped with the acting persona in
    // metadata (whose attribution must NOT survive).
    fold(
        &p,
        4,
        TYPE_DECISION_MADE,
        serde_json::json!({ "id": "d1", "summary": "use the shared buffer authority", "governs": ["src/combat.rs"], "supersedes": "" }),
        Some("rust-engineer"),
    );
    // A review finding about the file - CONTENT (survives, with its `by` reviewer kept as a node
    // ATTRIBUTE) stamped with the acting reviewer in metadata (whose attribution must NOT survive).
    fold(
        &p,
        5,
        TYPE_REVIEW_FINDING,
        serde_json::json!({ "id": "f1", "by": "tech-lens", "unit": "u1", "summary": "skips the buffer authority", "about": ["src/combat.rs"] }),
        Some("tech-lens"),
    );
    // The unit integrated - carrying the `commit` key the log records but the fold no longer models.
    // The arm folds no KIND_UNIT node; its disposition-expiry side effect still runs (no upheld
    // finding here, so it invalidates nothing).
    fold(
        &p,
        6,
        TYPE_UNIT_INTEGRATED,
        serde_json::json!({ "id": "u1", "commit": "abc1234" }),
        None,
    );

    // Seed the projection with every id the run named - the content ids AND the machinery ids - and
    // read a shallow neighborhood. A machinery id that never became a node contributes nothing, so
    // the presence of an agent / unit / gate node (or a machinery edge) can only mean the fold
    // projected it. Under the OLD fold the decision's actor and the finding's raiser would be
    // reached at depth 1 via DECIDED / RAISED, and the gate via GATED_BY off the file.
    let seeds: Vec<String> = [
        "d1",
        "f1",
        "src/combat.rs",
        "u1",
        "u0",
        "fmt",
        "rust-engineer",
        "tech-lens",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let g = p.subgraph(&seeds, 2).unwrap();

    // --- The machinery is GONE. ---
    assert!(
        !g.nodes.iter().any(|n| n.kind == KIND_AGENT),
        "no KIND_AGENT node is projected (the acting persona is not the target project); got {:?}",
        g.nodes.iter().map(|n| (&n.id, &n.kind)).collect::<Vec<_>>()
    );
    assert!(
        !g.nodes.iter().any(|n| n.kind == KIND_UNIT),
        "no KIND_UNIT node is projected (a unit is run machinery); got {:?}",
        g.nodes.iter().map(|n| (&n.id, &n.kind)).collect::<Vec<_>>()
    );
    assert!(
        !g.nodes.iter().any(|n| n.kind == KIND_GATE),
        "no KIND_GATE node is projected (a gate is run machinery); got {:?}",
        g.nodes.iter().map(|n| (&n.id, &n.kind)).collect::<Vec<_>>()
    );
    for rel in [
        REL_TOUCHES,
        REL_GATED_BY,
        REL_ASSIGNED_TO,
        REL_BLOCKS,
        REL_DECIDED,
        REL_RAISED,
    ] {
        assert!(
            !g.edges.iter().any(|e| e.rel == rel),
            "no {rel} machinery edge is projected; got {:?}",
            g.edges
                .iter()
                .map(|e| (&e.from, &e.rel, &e.to))
                .collect::<Vec<_>>()
        );
    }

    // --- The content SURVIVES. ---
    assert!(
        g.nodes
            .iter()
            .any(|n| n.id == "d1" && n.kind == KIND_DECISION),
        "the decision content node survives"
    );
    assert!(
        g.edges
            .iter()
            .any(|e| e.rel == REL_GOVERNS && e.from == "d1" && e.to == "src/combat.rs"),
        "the decision's GOVERNS edge to the code it concerns survives"
    );
    let f1 = g
        .nodes
        .iter()
        .find(|n| n.id == "f1")
        .expect("the finding content node survives");
    assert_eq!(f1.kind, KIND_FINDING, "and it is a KIND_FINDING node");
    assert_eq!(
        f1.attrs.get("by").map(String::as_str),
        Some("tech-lens"),
        "the finding's `by` reviewer survives as a node ATTRIBUTE, not as a KIND_AGENT node"
    );
    assert_eq!(
        f1.attrs.get("unit").map(String::as_str),
        Some("u1"),
        "the finding's owning-unit token survives as a node ATTRIBUTE (a string, not a KIND_UNIT node)"
    );
    assert!(
        g.edges
            .iter()
            .any(|e| e.rel == REL_ABOUT && e.from == "f1" && e.to == "src/combat.rs"),
        "the finding's ABOUT edge to the code it concerns survives"
    );
}

#[test]
fn the_actor_metadata_on_a_decision_and_finding_never_folds_an_agent_node() {
    // The focused regression pin on the single deleted code path most at risk of creeping back: the
    // old fold read `e.meta.get(META_ACTOR)` and, for BOTH a decision and a finding, materialized a
    // KIND_AGENT node plus a DECIDED / RAISED attribution edge. Here the decision's ONLY provenance
    // signal is the actor metadata (no `by`), and the finding's actor DIFFERS from its `by` (the old
    // code preferred the actor over `by` as the raiser). The de-noise must drop the actor entirely
    // while keeping the finding's `by` as content - so no agent node exists for the actor OR the
    // `by`, no DECIDED / RAISED edge is folded, and `by` stays exactly the reviewer, never the actor.
    let p = Projector::open(":memory:", "test").unwrap();
    fold(
        &p,
        1,
        TYPE_DECISION_MADE,
        serde_json::json!({ "id": "d1", "summary": "x", "governs": ["x.rs"], "supersedes": "" }),
        Some("rust-engineer"),
    );
    fold(
        &p,
        2,
        TYPE_REVIEW_FINDING,
        serde_json::json!({ "id": "f1", "by": "tech-lens", "unit": "u1", "summary": "y", "about": ["x.rs"] }),
        Some("adversary"),
    );

    let seeds: Vec<String> = [
        "d1",
        "f1",
        "x.rs",
        "rust-engineer",
        "tech-lens",
        "adversary",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let g = p.subgraph(&seeds, 2).unwrap();

    assert!(
        !g.nodes.iter().any(|n| n.kind == KIND_AGENT),
        "the actor metadata folds NO KIND_AGENT node for the actor or the `by`; got {:?}",
        g.nodes.iter().map(|n| (&n.id, &n.kind)).collect::<Vec<_>>()
    );
    assert!(
        !g.edges.iter().any(|e| e.rel == REL_DECIDED),
        "the decision's actor folds NO DECIDED attribution edge"
    );
    assert!(
        !g.edges.iter().any(|e| e.rel == REL_RAISED),
        "the finding's actor folds NO RAISED attribution edge"
    );
    // The content still stands, and `by` is the reviewer - never overwritten by the differing actor.
    assert!(
        g.edges
            .iter()
            .any(|e| e.rel == REL_GOVERNS && e.from == "d1" && e.to == "x.rs"),
        "the decision's GOVERNS content edge survives the actor drop"
    );
    let f1 = g
        .nodes
        .iter()
        .find(|n| n.id == "f1")
        .expect("the finding content node survives");
    assert_eq!(
        f1.attrs.get("by").map(String::as_str),
        Some("tech-lens"),
        "the finding's `by` is the reviewer content, not the differing event actor"
    );
}
