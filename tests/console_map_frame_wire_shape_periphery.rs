//! Periphery (contract) test for spec 84 criterion 1's map engine WIRE OUTPUT: `map_frame`'s
//! `DrawList` (and its nested `DrawDistrict`/`DrawEntity`/`DrawEdge`) is the one shape a
//! WebAssembly host's JS caller ever actually parses - `console-core`'s `map_frame` op
//! serializes it straight to JSON and hands the bytes back through the packed ABI reply (spec
//! 93's own module). Every one of `console::map`'s own unit tests (`src/console/map.rs`'s `mod
//! tests`) reads the RUST struct's fields directly (`.purpose`, `.name`, `.rel`, ...) - never
//! through `serde_json` - so a `#[serde(rename)]` slip, an accidentally-renamed field, or a
//! field that silently stopped deriving `Serialize` would still compile clean and pass every
//! one of those tests while breaking every JS caller that pattern-matches these exact JSON
//! keys. This file proves the wire shape directly: the field names `DrawList` actually emits,
//! and that a built frame round-trips through `serde_json` without losing a value - the one
//! property no Rust-side field access can prove on its own.
//!
//! A `DrawList` is recomputed fresh on every `map_frame` call - never persisted to, or replayed
//! from, the event store - so "back-compat" in the "an older writer's bytes must still parse
//! under a newer reader" sense the event-log's own `TYPE_` constants carry does not apply to it;
//! the periphery-appropriate proof for a transient wire shape like this one is that its CURRENT
//! shape is exactly what it claims to be, locked down here so a future field rename becomes a
//! test failure instead of a silent JS-side break.

use rigger::console::map;
use rigger::contextgraph::{
    Edge, Graph, Node, KIND_CODE_ENTITY, REL_CALLS, REL_IN_COMMUNITY, TIER_EXTRACTED,
};
use rigger::eventstore::Position;

fn code_node(id: &str, name: &str, kind: &str) -> Node {
    let attrs = [
        ("name".to_string(), name.to_string()),
        ("kind".to_string(), kind.to_string()),
    ]
    .into_iter()
    .collect();
    Node {
        id: id.to_string(),
        kind: KIND_CODE_ENTITY.to_string(),
        attrs,
    }
}

/// One district, two entities in it connected by a `CALLS` edge - the minimum fixture that
/// exercises all three `DrawList` members (`districts`, `entities`, `edges`) at once. Builds
/// `Edge`s inline (rather than through a standalone named helper) so this fixture's shape stays
/// its own, never a near-duplicate of `console::map`'s own `mod tests` fixture builders it has
/// no access to across the crate/integration-test boundary anyway.
fn two_entity_graph() -> Graph {
    let membership = |entity: &str| Edge {
        from: entity.to_string(),
        to: "community/1/0".to_string(),
        rel: REL_IN_COMMUNITY.to_string(),
        valid_from: 0,
        valid_to: None,
        source: Position::default(),
        tier: TIER_EXTRACTED.to_string(),
    };
    Graph {
        nodes: vec![
            code_node("src/a.rs::caller", "caller", "function"),
            code_node("src/a.rs::callee", "callee", "function"),
        ],
        edges: vec![
            membership("src/a.rs::caller"),
            membership("src/a.rs::callee"),
            Edge {
                from: "src/a.rs::caller".to_string(),
                to: "src/a.rs::callee".to_string(),
                rel: REL_CALLS.to_string(),
                valid_from: 0,
                valid_to: None,
                source: Position::default(),
                tier: TIER_EXTRACTED.to_string(),
            },
        ],
    }
}

/// `DrawList`'s three top-level fields, and each nested type's own fields, are EXACTLY what
/// this test names - never more, never fewer - the wire contract a JS caller parses.
#[test]
fn draw_list_json_carries_exactly_its_documented_field_names() {
    let graph = two_entity_graph();
    let model = map::build(&graph);
    let draw = map::frame(&model, 1400.0, 900.0, 0.0);

    assert_eq!(draw.districts.len(), 1, "{draw:?}");
    assert_eq!(draw.entities.len(), 2, "{draw:?}");
    assert_eq!(draw.edges.len(), 1, "{draw:?}");

    let json = serde_json::to_value(&draw).expect("DrawList must serialize to JSON");
    let obj = json
        .as_object()
        .expect("DrawList must serialize to a JSON object");
    let mut top_keys: Vec<&str> = obj.keys().map(String::as_str).collect();
    top_keys.sort_unstable();
    assert_eq!(
        top_keys,
        vec!["districts", "edges", "entities"],
        "DrawList's own top-level JSON keys must be exactly these three: {obj:?}"
    );

    let district = obj["districts"][0]
        .as_object()
        .expect("a district must serialize to a JSON object");
    let mut district_keys: Vec<&str> = district.keys().map(String::as_str).collect();
    district_keys.sort_unstable();
    assert_eq!(
        district_keys,
        vec!["population", "purpose", "radius", "x", "y"],
        "DrawDistrict's own JSON keys must be exactly these five: {district:?}"
    );

    let entity = obj["entities"][0]
        .as_object()
        .expect("an entity must serialize to a JSON object");
    let mut entity_keys: Vec<&str> = entity.keys().map(String::as_str).collect();
    entity_keys.sort_unstable();
    assert_eq!(
        entity_keys,
        vec!["id", "kind", "name", "x", "y"],
        "DrawEntity's own JSON keys must be exactly these five: {entity:?}"
    );

    let edge = obj["edges"][0]
        .as_object()
        .expect("an edge must serialize to a JSON object");
    let mut edge_keys: Vec<&str> = edge.keys().map(String::as_str).collect();
    edge_keys.sort_unstable();
    assert_eq!(
        edge_keys,
        vec!["from", "rel", "to"],
        "DrawEdge's own JSON keys must be exactly these three: {edge:?}"
    );
}

/// The same `DrawList` round-trips through `serde_json` without losing or mangling a single
/// value - the actual content a JS caller reads, not just the key names the test above locks
/// down.
#[test]
fn draw_list_json_round_trips_every_value_the_rust_struct_carries() {
    let graph = two_entity_graph();
    let model = map::build(&graph);
    let draw = map::frame(&model, 1400.0, 900.0, 0.0);

    let json = serde_json::to_value(&draw).expect("DrawList must serialize to JSON");

    assert_eq!(
        json["districts"][0]["purpose"].as_str().unwrap(),
        draw.districts[0].purpose,
        "{json}"
    );
    assert_eq!(
        json["districts"][0]["population"].as_u64().unwrap(),
        draw.districts[0].population as u64,
        "{json}"
    );

    let names: std::collections::BTreeSet<&str> =
        draw.entities.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        std::collections::BTreeSet::from(["caller", "callee"]),
        "both entities must survive the round trip with their own real names: {json}"
    );
    let json_names: std::collections::BTreeSet<&str> = json["entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names, json_names,
        "the JSON entity names must match the Rust struct's own names exactly: {json}"
    );

    assert_eq!(
        json["edges"][0]["from"].as_str().unwrap(),
        draw.edges[0].from,
        "{json}"
    );
    assert_eq!(
        json["edges"][0]["to"].as_str().unwrap(),
        draw.edges[0].to,
        "{json}"
    );
    assert_eq!(
        json["edges"][0]["rel"].as_str().unwrap(),
        draw.edges[0].rel,
        "{json}"
    );
    assert_eq!(
        draw.edges[0].rel, REL_CALLS,
        "the fixture's own CALLS edge must survive build()+frame() unchanged"
    );
}
