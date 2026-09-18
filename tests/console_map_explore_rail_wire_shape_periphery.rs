//! Periphery (contract) test for spec 84 criterion 2's own EXPLORE RAIL / SEARCH wire output:
//! `RailCandidate` (every `map_landmarks`/`map_bridges`/`map_changing`/`map_argued_about` chip)
//! and `SearchHit` (every `map_search` result row) are the two row shapes a WebAssembly host's
//! JS caller parses out of `graph_query`'s own `map_*` kind replies - `console-core`'s
//! `op_map_query` wraps a `Vec` of one or the other in a `{"candidates": [...]}` or
//! `{"hits": [...]}` envelope and serializes it straight to JSON (see that fn's own doc in
//! `crates/console-core/src/lib.rs`).
//!
//! `console::map`'s own unit tests read these structs' fields directly (`.id`, `.name`, ...) -
//! never through `serde_json` - so a `#[serde(rename)]` slip or an accidentally-renamed field
//! would still compile clean and pass every one of those tests. `exported_abi_periphery.rs`'s
//! own `map_landmarks`/`map_bridges`/`map_argued_about`/`map_changing`/`map_search` tests are
//! ALSO structurally blind to it: they compare a real `console_call` reply's LENGTH against
//! `RailCandidate`/`SearchHit`'s own `Serialize` impl called a second time, so a rename would
//! shift both sides of that comparison identically and the test would still pass. This file is
//! the one place that hardcodes the actual field names as string literals - the same technique
//! `console_map_frame_wire_shape_periphery.rs` already uses for `DrawList` - so a rename becomes
//! a test failure here instead of a silent JS-side break.

use rigger::console::map;
use rigger::contextgraph::{Edge, Graph, Node, KIND_CODE_ENTITY, REL_IN_COMMUNITY, TIER_EXTRACTED};
use rigger::eventstore::Position;

/// One bare entity, its own community - enough for [`map::landmarks`] and [`map::search`] to
/// each answer exactly one row, which is all a field-name test needs (the ROW COUNT and RANKING
/// these functions compute is `console::map`'s own `mod tests` job, not this file's).
fn one_entity_graph() -> Graph {
    Graph {
        nodes: vec![Node {
            id: "src/worktree.rs::hub".to_string(),
            kind: KIND_CODE_ENTITY.to_string(),
            attrs: [
                ("name".to_string(), "hub".to_string()),
                ("kind".to_string(), "function".to_string()),
            ]
            .into_iter()
            .collect(),
        }],
        edges: vec![Edge {
            from: "src/worktree.rs::hub".to_string(),
            to: "community/1/0".to_string(),
            rel: REL_IN_COMMUNITY.to_string(),
            valid_from: 0,
            valid_to: None,
            source: Position::default(),
            tier: TIER_EXTRACTED.to_string(),
        }],
    }
}

fn json_object_keys(v: &serde_json::Value) -> Vec<String> {
    let mut keys: Vec<String> = v
        .as_object()
        .unwrap_or_else(|| panic!("expected a JSON object, got {v:?}"))
        .keys()
        .cloned()
        .collect();
    keys.sort_unstable();
    keys
}

/// Serialize the sole row `rows` must carry (a field-name test's own setup gate: exactly one
/// row, never zero or many) to JSON, and assert the `id`/`name`/`kind` fields [`RailCandidate`]
/// and [`SearchHit`] carry IN COMMON - the ONE assertion body both of this file's tests share -
/// before handing back the JSON `Value` for the caller's own remaining, TYPE-SPECIFIC
/// assertions: the exact key list (different per type - `SearchHit` carries one more field), and
/// `SearchHit`'s own extra `degree`.
fn one_row_json_with_common_fields<T: serde::Serialize>(rows: Vec<T>) -> serde_json::Value {
    assert_eq!(
        rows.len(),
        1,
        "test setup gate: the fixture must answer exactly one row"
    );
    let json = serde_json::to_value(&rows[0]).expect("the row must serialize to JSON");
    assert_eq!(json["id"], "src/worktree.rs::hub", "{json:?}");
    assert_eq!(json["name"], "hub", "{json:?}");
    assert_eq!(json["kind"], "function", "{json:?}");
    json
}

/// `RailCandidate`'s own JSON keys are EXACTLY what this test names - never more, never fewer -
/// the wire contract every Explore rail chip's JS caller parses.
#[test]
fn rail_candidate_json_carries_exactly_its_documented_field_names() {
    let model = map::build(&one_entity_graph());
    let json = one_row_json_with_common_fields(map::landmarks(&model, 10));
    assert_eq!(
        json_object_keys(&json),
        vec!["id", "kind", "name"],
        "RailCandidate's own JSON keys must be exactly these three: {json:?}"
    );
}

/// `SearchHit`'s own JSON keys are EXACTLY what this test names - never more, never fewer - the
/// wire contract `map_search`'s JS caller parses. A distinct row shape from `RailCandidate`'s own
/// (an extra `degree` field, Design's own "kind and degree beside each hit"), proven separately
/// rather than assumed identical.
#[test]
fn search_hit_json_carries_exactly_its_documented_field_names() {
    let model = map::build(&one_entity_graph());
    let json = one_row_json_with_common_fields(map::search(&model, "hub", 10));
    assert_eq!(
        json_object_keys(&json),
        vec!["degree", "id", "kind", "name"],
        "SearchHit's own JSON keys must be exactly these four: {json:?}"
    );
    assert_eq!(json["degree"], 1, "{json:?}");
}
