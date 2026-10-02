//! Periphery (contract) test for spec 84 criterion 3's OTHER new pub item, `kind_colour`, and
//! for `legend`'s own wire COLOUR VALUES (not just its field names).
//!
//! `console::map`'s own unit tests prove `kind_colour` only from INSIDE its own compilation
//! unit: distinctness across the four named kinds, stability across calls, and the neutral
//! default for a kind outside them - all read as plain Rust values, never through an external
//! caller the way this criterion's other new pub items already are. `legend`'s own rows only
//! ever call `kind_colour` for the FOUR kinds `KIND_COLOURS` names; a real `DrawEntity::kind`
//! (`method`, `module`, an unrecognised future kind, ...) never reaches `kind_colour` through
//! any existing test, so the neutral-default fallback this module's own doc promises
//! ("once a page paints one, the canvas share[s]" this exact lookup) is exercised only inside
//! `map.rs`'s own module - never by an external Rust consumer the way `map::legend` already is
//! in `console_map_legend_wire_shape_periphery.rs`.
//!
//! `console_map_legend_wire_shape_periphery.rs` also proves `LegendEntry`'s JSON KEYS only -
//! never the wire VALUES. A `colour` swapped between two kind rows (or a blast-radius `colour`
//! quietly diverging from `BLAST_RADIUS_COLOUR`) would still carry the exact same key set and
//! the exact same envelope length the existing tests check, so neither would catch it.
//!
//! This file closes both gaps: `kind_colour` driven against a REAL graph's REAL `DrawEntity`
//! output (the actual future-canvas-consumer scenario, not a synthetic string), with hardcoded
//! literal colours rather than a second copy of `kind_colour`'s own lookup logic; and
//! `legend`'s wire `colour` values checked against `kind_colour`/`BLAST_RADIUS_COLOUR` read
//! independently, never against `LegendEntry`'s own field (which would only prove `serde`
//! didn't mangle a string - already implied by the existing key-set test).

use rigger::console::map;
use rigger::contextgraph::{Edge, Graph, Node, KIND_CODE_ENTITY, REL_IN_COMMUNITY, TIER_EXTRACTED};
use rigger::eventstore::Position;

/// Six entities in one community: Design's own four named kinds (`function`, `type`, `trait`,
/// `constant`) plus two real kinds `kind_colour` has no dedicated palette entry for (`method`,
/// `module` - see `KIND_COLOUR_DEFAULT`'s own doc), so `build`/`frame`'s real output actually
/// exercises both the named-palette branch and the neutral-default branch, never just one.
fn six_kind_graph() -> Graph {
    let kinds = ["function", "type", "trait", "constant", "method", "module"];
    Graph {
        nodes: kinds
            .iter()
            .map(|kind| Node {
                id: format!("src/fixture.rs::{kind}"),
                kind: KIND_CODE_ENTITY.to_string(),
                attrs: [
                    ("name".to_string(), format!("{kind}_entity")),
                    ("kind".to_string(), kind.to_string()),
                ]
                .into_iter()
                .collect(),
            })
            .collect(),
        edges: kinds
            .iter()
            .map(|kind| Edge {
                from: format!("src/fixture.rs::{kind}"),
                to: "community/1/0".to_string(),
                rel: REL_IN_COMMUNITY.to_string(),
                valid_from: 0,
                valid_to: None,
                source: Position::default(),
                tier: TIER_EXTRACTED.to_string(),
            })
            .collect(),
    }
}

/// `kind_colour`, called from outside `map.rs`'s own compilation unit, against every real
/// `DrawEntity::kind` a built-and-framed graph actually ships - the FOUR named kinds get their
/// exact documented swatch (hardcoded here, never re-derived from `KIND_COLOURS`, so a swapped
/// or renamed literal fails here even though the internal table that produced it is unchanged);
/// the two kinds outside the palette get the one neutral default, proven against a real wire
/// kind string rather than a hand-picked one.
#[test]
fn kind_colour_paints_every_real_draw_entity_kind_a_built_graph_produces() {
    let model = map::build(&six_kind_graph());
    let draw = map::frame(
        &model,
        1400.0,
        900.0,
        &map::Camera {
            cx: 0.0,
            cy: 0.0,
            zoom: 2.0,
        },
        None,
    );
    assert_eq!(
        draw.entities.len(),
        6,
        "test setup gate: the fixture must survive build/frame with all six entities: {:?}",
        draw.entities
    );

    for entity in &draw.entities {
        let expected = match entity.kind.as_str() {
            "function" => "#6ea8fe",
            "type" => "#4ade80",
            "trait" => "#a78bfa",
            "constant" => "#f472b6",
            _ => map::KIND_COLOUR_DEFAULT,
        };
        assert_eq!(
            map::kind_colour(&entity.kind),
            expected,
            "entity {:?} of kind {:?} must paint {expected}: {:?}",
            entity.name,
            entity.kind,
            draw.entities
        );
    }

    // Test setup gate: confirm the two out-of-palette kinds actually SURVIVED into the real
    // draw list - never a vacuously-true loop above where every entity happened to be one of
    // the four named kinds because the other two silently dropped out of build/frame.
    let outside_kinds: std::collections::BTreeSet<&str> = draw
        .entities
        .iter()
        .map(|e| e.kind.as_str())
        .filter(|k| !["function", "type", "trait", "constant"].contains(k))
        .collect();
    assert_eq!(
        outside_kinds,
        std::collections::BTreeSet::from(["method", "module"]),
        "test setup gate: the fixture must carry exactly these two out-of-palette kinds in the \
         real draw list: {:?}",
        draw.entities
    );
}

/// `legend`'s own wire `colour` VALUES (never checked anywhere else - the existing wire-shape
/// test pins field KEYS only) against the SAME independent public authorities a real canvas
/// consumer would call: each `entity-<kind>` row's colour must be `kind_colour(kind)`'s own
/// answer, the `blast-radius` row's colour must be `BLAST_RADIUS_COLOUR` itself, and every
/// other row (no fixed swatch) must carry JSON `null`, never an omitted field or a stray
/// string.
#[test]
fn legend_json_colour_values_match_their_independent_public_authorities() {
    let entries = map::legend();
    let json = serde_json::to_value(&entries).expect("legend rows must serialize to JSON");
    let rows = json
        .as_array()
        .expect("legend() must serialize to a JSON array");
    assert_eq!(rows.len(), entries.len(), "{rows:?}");

    for (row, entry) in rows.iter().zip(&entries) {
        if let Some(kind) = entry.id.strip_prefix("entity-") {
            assert_eq!(
                row["colour"].as_str(),
                Some(map::kind_colour(kind)),
                "the wire colour for {kind} must be the SAME independent kind_colour(...) \
                 answers: {row:?}"
            );
        } else if entry.id == "blast-radius" {
            assert_eq!(
                row["colour"].as_str(),
                Some(map::BLAST_RADIUS_COLOUR),
                "the wire blast-radius colour must be the SAME independent public constant: \
                 {row:?}"
            );
        } else {
            assert_eq!(
                row["colour"],
                serde_json::Value::Null,
                "{} carries no fixed swatch, so its wire colour must be JSON null, not \
                 omitted or a stray string: {row:?}",
                entry.id
            );
        }
    }
}
