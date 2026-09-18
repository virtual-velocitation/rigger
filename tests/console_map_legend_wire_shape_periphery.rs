//! Periphery (contract) test for spec 84 criterion 3's own LEGEND wire output: `LegendEntry`
//! (every row `console::map::legend` answers, and `graph_query`'s `map_legend` kind wraps in a
//! `{"entries": [...]}` envelope - see `op_graph_query`'s own doc in
//! `crates/console-core/src/lib.rs`) is the row shape a WebAssembly host's JS caller parses to
//! paint the persistent legend.
//!
//! `console::map`'s own unit tests read `LegendEntry`'s fields directly (`.id`, `.colour`, ...) -
//! never through `serde_json` - so a `#[serde(rename)]` slip or an accidentally-renamed field
//! would still compile clean and pass every one of those tests. This file is the one place that
//! hardcodes the actual field names as string literals - the same technique
//! `console_map_frame_wire_shape_periphery.rs` and `console_map_explore_rail_wire_shape_periphery.rs`
//! already use for `DrawList`/`RailCandidate`/`SearchHit` - so a rename becomes a test failure
//! here instead of a silent JS-side break. The key-set comparison itself is `tests/common`'s
//! shared `json_object_keys`, never a second copy of that helper (spec 85's own mandatory
//! duplication sweep: this file is the SECOND wire-shape suite that needs it, so it moved to the
//! shared module rather than being pasted again).

mod common;

use common::json_object_keys;
use rigger::console::map;

/// `LegendEntry`'s own JSON keys are EXACTLY what this test names - never more, never fewer -
/// the wire contract the legend's JS caller parses, checked on every row (not just one), since
/// `colour` is `Some` on some rows and `None` on others and both must still carry all four keys
/// (never an omitted `null` field - `serde`'s default struct behaviour, pinned here rather than
/// assumed).
#[test]
fn legend_entry_json_carries_exactly_its_documented_field_names_on_every_row() {
    for entry in map::legend() {
        let json = serde_json::to_value(&entry).expect("a LegendEntry must serialize to JSON");
        assert_eq!(
            json_object_keys(&json),
            vec!["colour", "id", "label", "treatment"],
            "LegendEntry's own JSON keys must be exactly these four for every row, including \
             {:?} whose colour is {:?}: {json:?}",
            entry.id,
            entry.colour
        );
    }
}

/// `graph_query`'s own `{"entries": [...]}` envelope (Design, LEGEND) carries the SAME rows, in
/// the SAME order, `console::map::legend` answers directly - proven at the JSON level so an
/// envelope-key rename (`entries` -> anything else) is caught here too, not only by
/// `exported_abi_periphery.rs`'s length-only comparison.
#[test]
fn legend_envelope_shape_matches_the_librarys_own_rows_in_order() {
    let rows = map::legend();
    let envelope = serde_json::json!({ "entries": rows });
    let entries = envelope["entries"]
        .as_array()
        .expect("the envelope must carry an entries array");
    assert_eq!(entries.len(), rows.len());
    for (row, wired) in rows.iter().zip(entries) {
        assert_eq!(wired["id"], row.id, "{wired:?}");
    }
}
