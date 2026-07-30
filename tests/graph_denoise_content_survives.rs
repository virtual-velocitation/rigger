//! Periphery (contract / API / integration) test for spec 43, criterion 2: the de-noised fold
//! still models the target project's DESIGN MEMORY. After the machinery drop, the same fold that
//! stops projecting agent / unit / gate nodes must STILL produce the content the graph is about -
//! the `KIND_DECISION` / `KIND_FINDING` / `KIND_LESSON` nodes and their `GOVERNS` / `ABOUT` /
//! `explains` edges to the code AND design they concern - with only the acting persona's
//! attribution absent. This test OWNS criterion 2 (content preservation); it deliberately does
//! NOT own the machinery drop (criterion 1) - it asserts only the ONE machinery fact criterion 2
//! names, "only the agent attribution is absent", so the two criteria never share an authority.
//!
//! It runs OUTSIDE the crate, over the library's public surface - `Projector::open` ->
//! `Projection::apply` -> `Projection::subgraph` - so it pins the projection a real consumer (the
//! dash inspector, grounding, blast-radius, the rationale overlay) actually reads. Two facets are
//! load-bearing and could regress if the de-noise ever over-reached:
//!   * a LESSON and a design-intent `explains` link (spec 29b) fold in the SAME mixed stream as the
//!     decision and finding, so the test proves the whole design-memory half survives, not just the
//!     two content kinds the machinery-drop test happens to touch; and
//!   * the decision and finding carry a non-empty actor in event metadata (`META_ACTOR`), exactly
//!     as the conductor stamps every real emit - the precise input the deleted `e.meta.get(...)`
//!     code once turned into a `KIND_AGENT` node and a `DECIDED` / `RAISED` edge. The content must
//!     stand while that attribution stays gone.

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    Projection, KIND_AGENT, KIND_DECISION, KIND_FINDING, KIND_LESSON, KIND_RATIONALE, META_ACTOR,
    REL_ABOUT, REL_DECIDED, REL_EXPLAINS, REL_GOVERNS, REL_RAISED, TYPE_DECISION_MADE,
    TYPE_DOC_CONCEPT_EXTRACTED, TYPE_DOC_LINK_EXTRACTED, TYPE_FILE_TOUCHED, TYPE_GATE_VERDICT,
    TYPE_LESSON_LEARNED, TYPE_REVIEW_FINDING, TYPE_UNIT_INTEGRATED, TYPE_UNIT_STARTED,
};
use rigger::eventstore::Event;

/// Fold one event from its raw on-log JSON at `pos`, optionally stamping the acting persona in
/// `META_ACTOR` (the metadata the conductor puts on every real emit). Folding the raw JSON - not an
/// in-crate payload struct - pins the contract the log actually carries. `apply` returns `Err` on a
/// fold failure, so a successful call is itself evidence the payload folded.
fn fold(p: &Projector, pos: u64, type_: &str, payload: serde_json::Value, actor: Option<&str>) {
    let mut e = Event::new(type_, serde_json::to_vec(&payload).unwrap());
    e.position = pos;
    if let Some(a) = actor {
        e.meta.insert(META_ACTOR.to_string(), a.to_string());
    }
    p.apply(&e).unwrap();
}

#[test]
fn the_de_noised_fold_keeps_the_design_memory_and_drops_only_the_agent_attribution() {
    // Fold a whole run's mixed stream - the machinery arms (touched file, unit lifecycle, gate) AND
    // the design-memory arms (a decision, a finding, a lesson, and a design-intent rationale link) -
    // into one projection, each event in its raw production shape, the two content-carrying events
    // stamped with their acting persona in metadata. Then read the neighborhood a consumer crosses
    // through the public `subgraph`. Every content node and edge must be present; the acting persona
    // must not be a node and must not be attached by any attribution edge.
    let p = Projector::open(":memory:", "test").unwrap();

    // --- Machinery events in the stream (their de-noise to graph no-ops is criterion 1's to own;
    //     they are here only so the content is proven to survive a REAL mixed run, not a sterile
    //     content-only one). ---
    fold(
        &p,
        1,
        TYPE_FILE_TOUCHED,
        serde_json::json!({ "path": "src/combat.rs", "by": "rust-engineer" }),
        None,
    );
    fold(
        &p,
        2,
        TYPE_UNIT_STARTED,
        serde_json::json!({ "unit": "u1", "criterion": "c2", "agent": "rust-engineer", "needs": ["u0"] }),
        None,
    );
    fold(
        &p,
        3,
        TYPE_GATE_VERDICT,
        serde_json::json!({ "gate": "clippy", "pass": true, "artifact": "src/combat.rs" }),
        None,
    );

    // --- The design memory: the content the graph IS about. ---
    // A decision that GOVERNS both a CODE file and a DESIGN/spec doc - proving the content edge lands
    // on code AND design. Stamped with its acting persona, whose attribution must NOT survive.
    fold(
        &p,
        4,
        TYPE_DECISION_MADE,
        serde_json::json!({
            "id": "d1",
            "summary": "route the buffer through the shared authority",
            "governs": ["src/combat.rs", "specs/43-de-noise-knowledge-graph-to-target-project.md"],
            "supersedes": ""
        }),
        Some("rust-engineer"),
    );
    // A review finding ABOUT a code file, carrying its reviewer (`by`) and owning-unit token.
    // Stamped with a DIFFERENT acting persona than its `by`, so a creeping-back attribution would be
    // visible as either the actor or the `by` becoming a node.
    fold(
        &p,
        5,
        TYPE_REVIEW_FINDING,
        serde_json::json!({
            "id": "f1",
            "by": "tech-lens",
            "unit": "u1",
            "summary": "skips the shared buffer authority",
            "about": ["src/combat.rs"]
        }),
        Some("adversary"),
    );
    // A lesson ABOUT a code file - the third content kind criterion 2 names, which the machinery-drop
    // test does not co-locate. Its content node and ABOUT edge must survive intact.
    fold(
        &p,
        6,
        TYPE_LESSON_LEARNED,
        serde_json::json!({
            "id": "l1",
            "summary": "one mutation authority per buffer, never a reconciled second",
            "about": ["src/combat.rs"]
        }),
        None,
    );
    // A design-intent rationale node and its `explains` link to the code it annotates (spec 29b): the
    // "explains edges to ... design" half of criterion 2. The de-noise never touched this arm, so its
    // survival in a mixed fold proves the design-memory overlay stands beside the machinery drop.
    fold(
        &p,
        7,
        TYPE_DOC_CONCEPT_EXTRACTED,
        serde_json::json!({
            "kind": KIND_RATIONALE,
            "id": "src/combat.rs#L7",
            "title": "WHY: clamp to the shared buffer",
            "doc": "src/combat.rs"
        }),
        None,
    );
    fold(
        &p,
        8,
        TYPE_DOC_LINK_EXTRACTED,
        serde_json::json!({ "from": "src/combat.rs#L7", "rel": REL_EXPLAINS, "to": "src/combat.rs" }),
        None,
    );
    // The unit integrates - the log carries a `commit` key the de-noised fold no longer models. No
    // upheld finding is marked here, so disposition-expiry invalidates nothing and the content stands.
    fold(
        &p,
        9,
        TYPE_UNIT_INTEGRATED,
        serde_json::json!({ "id": "u1", "commit": "abc1234" }),
        None,
    );

    // Seed from every content id AND every acting persona / machinery id the run named, then read a
    // shallow neighborhood. A persona id that never became a node contributes nothing, so if an agent
    // node (or an attribution edge) shows up it can only be because the fold projected it.
    let seeds: Vec<String> = [
        "d1",
        "f1",
        "l1",
        "src/combat.rs",
        "src/combat.rs#L7",
        "specs/43-de-noise-knowledge-graph-to-target-project.md",
        "rust-engineer",
        "tech-lens",
        "adversary",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let g = p.subgraph(&seeds, 2).unwrap();

    let node = |id: &str, kind: &str| g.nodes.iter().any(|n| n.id == id && n.kind == kind);
    let edge = |from: &str, rel: &str, to: &str| {
        g.edges
            .iter()
            .any(|e| e.from == from && e.rel == rel && e.to == to)
    };

    // --- The DECISION content survives, GOVERNING both the code AND the design doc. ---
    assert!(
        node("d1", KIND_DECISION),
        "the decision content node survives; got {:?}",
        g.nodes.iter().map(|n| (&n.id, &n.kind)).collect::<Vec<_>>()
    );
    assert!(
        edge("d1", REL_GOVERNS, "src/combat.rs"),
        "the decision GOVERNS the code it concerns; got {:?}",
        g.edges
            .iter()
            .map(|e| (&e.from, &e.rel, &e.to))
            .collect::<Vec<_>>()
    );
    assert!(
        edge(
            "d1",
            REL_GOVERNS,
            "specs/43-de-noise-knowledge-graph-to-target-project.md"
        ),
        "the decision GOVERNS the design doc it concerns (content reaches DESIGN, not only code); got {:?}",
        g.edges.iter().map(|e| (&e.from, &e.rel, &e.to)).collect::<Vec<_>>()
    );

    // --- The FINDING content survives, with its reviewer and owning-unit kept as ATTRIBUTES. ---
    let f1 = g
        .nodes
        .iter()
        .find(|n| n.id == "f1")
        .expect("the finding content node survives");
    assert_eq!(f1.kind, KIND_FINDING, "and it is a KIND_FINDING node");
    assert_eq!(
        f1.attrs.get("by").map(String::as_str),
        Some("tech-lens"),
        "the finding's `by` reviewer survives as a node ATTRIBUTE (the reviewer, never the differing actor), not a KIND_AGENT node"
    );
    assert_eq!(
        f1.attrs.get("unit").map(String::as_str),
        Some("u1"),
        "the finding's owning-unit token survives as a node ATTRIBUTE (a string, not a KIND_UNIT node)"
    );
    assert!(
        edge("f1", REL_ABOUT, "src/combat.rs"),
        "the finding's ABOUT edge to the code it concerns survives; got {:?}",
        g.edges
            .iter()
            .map(|e| (&e.from, &e.rel, &e.to))
            .collect::<Vec<_>>()
    );

    // --- The LESSON content survives, ABOUT the code it concerns. ---
    assert!(
        node("l1", KIND_LESSON),
        "the lesson content node survives; got {:?}",
        g.nodes.iter().map(|n| (&n.id, &n.kind)).collect::<Vec<_>>()
    );
    assert!(
        edge("l1", REL_ABOUT, "src/combat.rs"),
        "the lesson's ABOUT edge to the code it concerns survives; got {:?}",
        g.edges
            .iter()
            .map(|e| (&e.from, &e.rel, &e.to))
            .collect::<Vec<_>>()
    );

    // --- The design-intent `explains` edge survives, to the code it annotates. ---
    assert!(
        node("src/combat.rs#L7", KIND_RATIONALE),
        "the rationale (design-intent) node survives; got {:?}",
        g.nodes.iter().map(|n| (&n.id, &n.kind)).collect::<Vec<_>>()
    );
    assert!(
        edge("src/combat.rs#L7", REL_EXPLAINS, "src/combat.rs"),
        "the rationale's `explains` edge to the code it annotates survives (design memory stands); got {:?}",
        g.edges.iter().map(|e| (&e.from, &e.rel, &e.to)).collect::<Vec<_>>()
    );

    // --- ONLY the agent attribution is absent (the ONE machinery fact criterion 2 owns; the rest of
    //     the machinery drop is criterion 1's to prove). No node for the acting persona of the
    //     decision or the finding, and no attribution edge tying either back to its actor. ---
    assert!(
        !g.nodes.iter().any(|n| n.kind == KIND_AGENT),
        "the acting persona folds NO KIND_AGENT node - the attribution is the only thing dropped; got {:?}",
        g.nodes.iter().map(|n| (&n.id, &n.kind)).collect::<Vec<_>>()
    );
    assert!(
        !g.edges.iter().any(|e| e.rel == REL_DECIDED),
        "the decision's actor folds NO DECIDED attribution edge; got {:?}",
        g.edges
            .iter()
            .map(|e| (&e.from, &e.rel, &e.to))
            .collect::<Vec<_>>()
    );
    assert!(
        !g.edges.iter().any(|e| e.rel == REL_RAISED),
        "the finding's actor folds NO RAISED attribution edge; got {:?}",
        g.edges
            .iter()
            .map(|e| (&e.from, &e.rel, &e.to))
            .collect::<Vec<_>>()
    );
}
