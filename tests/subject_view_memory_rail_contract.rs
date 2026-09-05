//! Periphery (API / contract) tests for spec 63 criterion 5 - the SUBJECT VIEW's docked MEMORY
//! RAIL. `memory_rail`, `MemoryRail`, and `ConceptRef` are the unit's new PUBLIC surface; this file
//! runs OUTSIDE the crate, over that surface only (`rigger::dash::{memory_rail, route, MemoryRail,
//! ConceptRef, RationaleLeaf}` plus `rigger::contextgraph::*`) - no `super::`, no crate-internal
//! visibility. Every field this file reads must genuinely be `pub` to even COMPILE, which is a
//! boundary the implementer's own `dash.rs` `subject_view_c5` tests (built with `use super::*`)
//! cannot prove: an accidental `pub(crate)` on a `MemoryRail` field, or on `memory_rail` itself,
//! would still compile and pass every one of those same-crate tests while breaking every external
//! consumer of the library (including this one, and the served route it feeds).
//!
//! Three things this layer proves that the inside-out tests are structurally blind to:
//!
//!  - the FOLD is genuinely reachable and correct from OUTSIDE the crate: a `Graph` built with only
//!    public `contextgraph` constructors, fed to the public `memory_rail`, yields a `MemoryRail`
//!    whose fields (and `ConceptRef`'s / `RationaleLeaf`'s) are all directly readable;
//!  - the WIRE CONTRACT: the public `route` function's `/api/graph?seed=` JSON body carries the
//!    rail's content under a `memory` key, over an actual serde round-trip (struct -> JSON string ->
//!    parsed value), reached only through `route`'s public signature;
//!  - the BACK-COMPAT / BYTE-IDENTITY claim the unit documents (`#[serde(skip_serializing_if =
//!    "Option::is_none")]`): a cluster drill and the lens-less whole-graph overview - two OTHER
//!    `/api/graph` response shapes the same route serves - carry no `memory` key at all, proven at
//!    the served body rather than trusted from the `Neighborhood { memory: None, .. }` construction
//!    site.
//!
//! `dash` and `contextgraph` compile on BOTH the default and `--no-default-features` lanes (nothing
//! here is feature-gated), so these tests run in both. No reference to any external tool or project;
//! hyphens, never em dashes.

use std::collections::HashMap;

use rigger::contextgraph::{
    Edge, Graph, Node, KIND_CODE_ENTITY, KIND_CONCEPT, KIND_DECISION, KIND_FILE, KIND_FINDING,
    KIND_LESSON, REL_ABOUT, REL_GOVERNS, REL_REALIZES, TIER_INFERRED,
};
use rigger::dash::{memory_rail, route, ConceptRef, MemoryRail, RationaleLeaf};

fn node(id: &str, kind: &str, attrs: &[(&str, &str)]) -> Node {
    Node {
        id: id.to_string(),
        kind: kind.to_string(),
        attrs: attrs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    }
}

fn edge(from: &str, to: &str, rel: &str) -> Edge {
    Edge {
        from: from.to_string(),
        to: to.to_string(),
        rel: rel.to_string(),
        valid_from: 0,
        valid_to: None,
        source: 0,
        tier: TIER_INFERRED.to_string(),
    }
}

/// The same narrative fixture the implementer's inside-out tests and the client harness use
/// (`combat.rs::fire`, `d1`/`f1`/`l1`, `concept/combat`) - built here from ONLY public `contextgraph`
/// constructors, so a subject with a governing decision, an ABOUT finding, an ABOUT lesson
/// (excluded from the rail), and its own live `REALIZES` edge to a concept.
fn subject_graph() -> Graph {
    Graph {
        nodes: vec![
            node("combat.rs::fire", KIND_CODE_ENTITY, &[]),
            node("other.rs", KIND_FILE, &[]),
            node(
                "d1",
                KIND_DECISION,
                &[("summary", "use the shared authority")],
            ),
            node("f1", KIND_FINDING, &[("summary", "the finding content")]),
            node("l1", KIND_LESSON, &[("summary", "the lesson content")]),
            node(
                "concept/combat",
                KIND_CONCEPT,
                &[("label", "combat resolution")],
            ),
        ],
        edges: vec![
            edge("d1", "combat.rs::fire", REL_GOVERNS),
            edge("f1", "combat.rs::fire", REL_ABOUT),
            edge("l1", "combat.rs::fire", REL_ABOUT),
            edge("combat.rs::fire", "concept/combat", REL_REALIZES),
        ],
    }
}

/// `memory_rail`/`MemoryRail`/`ConceptRef`/`RationaleLeaf` are genuinely public: this test reads
/// every field it asserts on through the crate's external boundary alone.
#[test]
fn memory_rail_is_reachable_and_correct_over_the_public_boundary() {
    let g = subject_graph();
    let rail: MemoryRail = memory_rail(&g, "combat.rs::fire");

    let decisions: Vec<&str> = rail
        .decisions
        .iter()
        .map(|d: &RationaleLeaf| d.id.as_str())
        .collect();
    assert_eq!(
        decisions,
        vec!["d1"],
        "the governing decision surfaces over the public boundary: {rail:?}"
    );
    assert_eq!(
        rail.decisions[0].summary, "use the shared authority",
        "the decision leaf carries its content: {rail:?}"
    );

    let findings: Vec<&str> = rail.findings.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(
        findings,
        vec!["f1"],
        "the ABOUT finding surfaces over the public boundary: {rail:?}"
    );

    let concepts: &[ConceptRef] = &rail.concepts;
    assert_eq!(concepts.len(), 1, "exactly one concept: {rail:?}");
    assert_eq!(concepts[0].id, "concept/combat");
    assert_eq!(concepts[0].label, "combat resolution");

    assert!(
        !rail
            .decisions
            .iter()
            .chain(rail.findings.iter())
            .any(|leaf| leaf.id == "l1"),
        "the lesson is excluded from both rail buckets over the public boundary too: {rail:?}"
    );
}

/// The served `/api/graph?seed=` route (the public `route` function) carries the rail as JSON,
/// reached only through a genuine serde round-trip - never a direct look at the `MemoryRail` value.
#[test]
fn a_seeded_route_response_carries_the_memory_rail_over_the_wire() {
    let g = subject_graph();
    let resp = route(
        "GET",
        "/api/graph?seed=combat.rs%3A%3Afire&depth=1",
        &[],
        &g,
        &[],
        &HashMap::new(),
        3,
        "rigger-run",
        "origin/main",
        &[],
    );
    assert_eq!(resp.status, 200, "a seeded request is served 200");
    let body: serde_json::Value =
        serde_json::from_slice(&resp.body).expect("the served body is valid JSON");

    assert_eq!(
        body["memory"]["decisions"][0]["id"], "d1",
        "the decision rides the wire: {body}"
    );
    assert_eq!(
        body["memory"]["findings"][0]["id"], "f1",
        "the finding rides the wire: {body}"
    );
    assert_eq!(
        body["memory"]["concepts"][0]["id"], "concept/combat",
        "the concept rides the wire: {body}"
    );
    assert_eq!(
        body["memory"]["concepts"][0]["label"], "combat resolution",
        "the concept's label rides the wire: {body}"
    );
    let no_lesson = body["memory"]["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .chain(body["memory"]["findings"].as_array().unwrap().iter())
        .all(|leaf| leaf["id"] != "l1");
    assert!(
        no_lesson,
        "the lesson never rides the wire either bucket: {body}"
    );
}

/// BACK-COMPAT proof at the served body (not the construction site): a cluster drill and the
/// lens-less whole-graph overview - the other two `/api/graph` response shapes this same route
/// serves - carry NO `memory` key at all, so neither existing consumer sees a shape change.
#[test]
fn a_cluster_drill_and_the_whole_graph_overview_carry_no_memory_field() {
    let g = subject_graph();
    let liveness = HashMap::new();

    let drill = route(
        "GET",
        "/api/graph?cluster=other.rs",
        &[],
        &g,
        &[],
        &liveness,
        3,
        "rigger-run",
        "origin/main",
        &[],
    );
    assert_eq!(drill.status, 200, "a cluster drill is served 200");
    let drill_body = String::from_utf8(drill.body).expect("a utf8 body");
    assert!(
        !drill_body.contains("\"memory\""),
        "a cluster drill carries no memory field over the wire: {drill_body}"
    );

    let overview = route(
        "GET",
        "/api/graph",
        &[],
        &g,
        &[],
        &liveness,
        3,
        "rigger-run",
        "origin/main",
        &[],
    );
    assert_eq!(
        overview.status, 200,
        "the whole-graph overview is served 200"
    );
    let overview_body = String::from_utf8(overview.body).expect("a utf8 body");
    assert!(
        !overview_body.contains("\"memory\""),
        "the whole-graph overview carries no memory field over the wire: {overview_body}"
    );
}
