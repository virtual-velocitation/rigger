//! Periphery (API / integration / contract) tests for spec 63's METADATA CARD (criterion 2): "one
//! hover-card anatomy everywhere" - a code subject's card carries its definition file:line, its
//! coupling community, the concepts it REALIZES, and decision/finding COUNTS; a file subject's
//! card lists its CONTAINED entities; a concept subject's card lists its REALIZING members. Every
//! other taxonomy than the card's own subject lives here as METADATA, never a second graph node.
//!
//! These run OUTSIDE the crate, over the library's PUBLIC surface (`rigger::dash::{Card, CardRef,
//! CardResponse, card, route, ...}`), so they guard the exact boundaries the inside-out unit test
//! (`src/dash.rs mod metadata_card_c2`, which reaches `card` via `super::` in-process) is
//! structurally blind to:
//!
//!  - PUBLIC REACHABILITY. The unit test proves the card BEHAVIOUR but never that `card` and its
//!    carrier DTOs (`Card`, `CardRef`, `CardResponse`) stay `pub` and reachable as
//!    `rigger::dash::...`. If any were accidentally crate-private, only a crate-external test
//!    fails to COMPILE - the inside-out test would stay green.
//!  - THE SERVED ROUTE END-TO-END, over the SAME `route` dispatch the browser hits on every
//!    `/api/graph?card=` request - not `card` called directly.
//!  - THE SERIALIZED WIRE-SHAPE back-compat: `Card.file` / `Card.line` / `Card.community` /
//!    `Card.top_entities` / `Card.top_evidence` are all `skip_serializing_if`, so a code entity's
//!    served card carries NO `top_entities`/`top_evidence` keys at all, and a membership-less
//!    entity's card carries NO `community` key. The unit test asserts Rust struct equality, never
//!    the JSON keys' presence/absence a hand-rolled JS client actually reads.
//!  - THE ROUTE's PRECEDENCE: `card=` must be recognized BEFORE the lens/seed dispatch, so a
//!    request carrying `card=` alongside a stray `seed=`/`lens=` (never sent by the served page,
//!    but a hostile or malformed URL could) still serves the card, never a neighborhood/reprojection
//!    body - a regression only the served `route` (never the pure `card` function) could exhibit.
//!
//! `dash` + `contextgraph` compile on BOTH the default and the `--no-default-features` lane
//! (neither the route nor these DTOs is feature-gated), so this guards the served contract in both
//! lanes.

use std::collections::HashMap;

use rigger::contextgraph::{
    Edge, Graph, Node, KIND_CODE_ENTITY, KIND_COMMUNITY, KIND_CONCEPT, KIND_DECISION, KIND_FILE,
    KIND_FINDING, REL_ABOUT, REL_CONTAINS, REL_GOVERNS, REL_IN_COMMUNITY, REL_REALIZES,
    TIER_INFERRED,
};
use rigger::dash::{card, route, Card, CardRef, CardResponse};

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

/// A code entity `combat.rs::fire` (line 42) carrying a live default-grain `IN_COMMUNITY`
/// membership, a `REALIZES` concept, a governing decision, and an ABOUT finding - one of every
/// taxonomy the card's rows carry - plus its own file `combat.rs` (with a second entity, so the
/// file's card lists more than one top entity) and the concept it realizes (so the concept's own
/// card lists it back as evidence).
fn fixture_graph() -> Graph {
    Graph {
        nodes: vec![
            node(
                "combat.rs::fire",
                KIND_CODE_ENTITY,
                &[("name", "fire"), ("kind", "function"), ("line", "42")],
            ),
            node("combat.rs::reload", KIND_CODE_ENTITY, &[("name", "reload")]),
            node("combat.rs", KIND_FILE, &[]),
            node(
                "community/1/3",
                KIND_COMMUNITY,
                &[("label", "combat lifecycle")],
            ),
            node(
                "concept/combat",
                KIND_CONCEPT,
                &[("label", "combat resolution")],
            ),
            node(
                "d1",
                KIND_DECISION,
                &[("summary", "use the shared authority")],
            ),
            node("f1", KIND_FINDING, &[("summary", "the finding content")]),
        ],
        edges: vec![
            edge("combat.rs", "combat.rs::fire", REL_CONTAINS),
            edge("combat.rs", "combat.rs::reload", REL_CONTAINS),
            edge("combat.rs::fire", "community/1/3", REL_IN_COMMUNITY),
            edge("combat.rs::fire", "concept/combat", REL_REALIZES),
            edge("d1", "combat.rs::fire", REL_GOVERNS),
            edge("f1", "combat.rs::fire", REL_ABOUT),
        ],
    }
}

/// `card` and its carrier DTOs stay `pub` and reachable as `rigger::dash::...` from OUTSIDE the
/// crate - the exact boundary the inside-out unit test cannot prove (a demoted-to-private item
/// would strand the in-module test green while every external consumer fails to compile).
#[test]
fn card_and_its_dtos_are_reachable_from_outside_the_crate() {
    let g = fixture_graph();
    let c: Card = card(&g, "combat.rs::fire").expect("a public Card DTO");
    assert_eq!(c.id, "combat.rs::fire");
    let _: &Vec<CardRef> = &c.top_entities;
    let body = CardResponse { card: Some(c) };
    assert!(body.card.is_some());
}

/// The served `/api/graph?card=<id>` route (never `card` called directly): a code entity's card
/// carries file:line, its community, the concept it realizes, and decision/finding COUNTS - the
/// exact behavioral shape a `/api/graph?card=` client request receives.
#[test]
fn the_served_route_carries_a_code_subjects_card() {
    let g = fixture_graph();
    let resp = route(
        "GET",
        "/api/graph?card=combat.rs%3A%3Afire",
        &[],
        &g,
        &[],
        &HashMap::new(),
        3,
        "rigger-run",
        "origin/main",
        &[],
    );
    assert_eq!(resp.status, 200);
    let body = String::from_utf8(resp.body).expect("a utf8 body");
    let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    let c = &json["card"];
    assert_eq!(c["id"].as_str(), Some("combat.rs::fire"), "{body}");
    assert_eq!(c["kind"].as_str(), Some("code-entity"), "{body}");
    assert_eq!(c["file"].as_str(), Some("combat.rs"), "{body}");
    assert_eq!(c["line"].as_str(), Some("42"), "{body}");
    assert_eq!(c["community"].as_str(), Some("combat lifecycle"), "{body}");
    assert_eq!(
        c["concepts"][0]["id"].as_str(),
        Some("concept/combat"),
        "{body}"
    );
    assert_eq!(c["decisions"].as_i64(), Some(1), "{body}");
    assert_eq!(c["findings"].as_i64(), Some(1), "{body}");
    assert!(
        c.get("top_entities").is_none(),
        "a code entity's served card carries NO top_entities key at all \
         (skip_serializing_if, never an empty array): {body}"
    );
}

/// The served route also serves a FILE subject's card (`top_entities`) and a CONCEPT subject's
/// card (`top_evidence`) - the same route arm, dispatched purely on the requested id's own kind.
#[test]
fn the_served_route_carries_a_file_and_a_concept_subjects_card() {
    let g = fixture_graph();

    let resp = route(
        "GET",
        "/api/graph?card=combat.rs",
        &[],
        &g,
        &[],
        &HashMap::new(),
        3,
        "rigger-run",
        "origin/main",
        &[],
    );
    let body = String::from_utf8(resp.body).expect("a utf8 body");
    let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    let mut ids: Vec<&str> = json["card"]["top_entities"]
        .as_array()
        .expect("a top_entities array")
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    ids.sort_unstable();
    assert_eq!(
        ids,
        vec!["combat.rs::fire", "combat.rs::reload"],
        "a file's card lists its CONTAINED entities: {body}"
    );
    assert!(
        json["card"].get("top_evidence").is_none(),
        "a file's card carries no top_evidence key: {body}"
    );

    let resp = route(
        "GET",
        "/api/graph?card=concept%2Fcombat",
        &[],
        &g,
        &[],
        &HashMap::new(),
        3,
        "rigger-run",
        "origin/main",
        &[],
    );
    let body = String::from_utf8(resp.body).expect("a utf8 body");
    let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert_eq!(
        json["card"]["top_evidence"][0]["id"].as_str(),
        Some("combat.rs::fire"),
        "a concept's card lists the members that REALIZE it: {body}"
    );
    assert!(
        json["card"].get("top_entities").is_none(),
        "a concept's card carries no top_entities key: {body}"
    );
}

/// An id absent from the graph serves `{"card":null}` at 200 - the graceful-empty contract every
/// `/api/graph` read keeps, never a 404/500 - proven over the SERVED route.
#[test]
fn the_served_route_degrades_gracefully_for_an_unknown_card_subject() {
    let g = fixture_graph();
    let resp = route(
        "GET",
        "/api/graph?card=not-a-node",
        &[],
        &g,
        &[],
        &HashMap::new(),
        3,
        "rigger-run",
        "origin/main",
        &[],
    );
    assert_eq!(resp.status, 200);
    let body = String::from_utf8(resp.body).expect("a utf8 body");
    assert_eq!(body, "{\"card\":null}", "{body}");
}

/// `card=` is recognized BEFORE the lens/seed dispatch: a request carrying `card=` ALONGSIDE a
/// stray `seed=`/`lens=` still serves the card body, never a neighborhood/reprojection - a
/// regression only the served `route`'s OWN precedence (never the pure `card` function) could
/// exhibit.
#[test]
fn the_card_param_takes_precedence_over_a_stray_seed_or_lens_param() {
    let g = fixture_graph();
    let resp = route(
        "GET",
        "/api/graph?card=combat.rs%3A%3Afire&seed=combat.rs&lens=files",
        &[],
        &g,
        &[],
        &HashMap::new(),
        3,
        "rigger-run",
        "origin/main",
        &[],
    );
    assert_eq!(resp.status, 200);
    let body = String::from_utf8(resp.body).expect("a utf8 body");
    let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert_eq!(
        json["card"]["id"].as_str(),
        Some("combat.rs::fire"),
        "card= must win over a stray seed=/lens=, never fall through to a reprojection: {body}"
    );
    assert!(
        json.get("subject").is_none() && json.get("nodes").is_none(),
        "the body must be a pure card response, not a neighborhood/reprojection shape: {body}"
    );
}
