//! Periphery (integration) tests for spec 117, PYTHON STAGES ITS ROW, across the seam the grounder
//! crate's `tests/facts_python.rs` stops short of: that suite pins the mentions the bundled Python
//! grammar stages; this one pins that the real `rigger graph build` lands a `const`, an `impl` and
//! a `field` mention in the graph through the resolver, each as one `REFERENCES` edge and never a
//! call:
//!
//!  - a module constant's read, bare or as the attribute `cfg.MAX` of a lowercase object, lands on
//!    that constant, and a base class on that class, each `extracted` onto its one same-file
//!    definition;
//!  - an attribute read `x.depth` lands on the one field `depth` names, `inferred` though that
//!    field is in the same file, `Shape.side` lands on `Shape`'s `side` over `Other`'s, and the
//!    member constant `Config.MAX` on `Config`'s `MAX` and never the module constant `MAX`, each
//!    `extracted`.
//!
//! The `type` mention of an annotation and the call named `Shape.area` land in the Python rows of
//! `references_are_extracted_and_isolated_periphery`'s `TYPE_USERS` and `QUALIFIED_FILES`, and the
//! docstring spans reach the code store in `doc_spans_reach_the_code_store_periphery`.
//!
//! Every test extracts real source, so the file runs on the `symbols` lane alone.

use crate::common;

use rigger::contextgraph::{TIER_EXTRACTED, TIER_INFERRED};

/// One Python file: what it shows, its source, the id paths of its definitions, sorted, and each
/// edge it lands as (from's id path, to's id path, tier).
type Landing = (
    &'static str,
    &'static str,
    &'static [&'static str],
    &'static [(&'static str, &'static str, &'static str)],
);

/// One [`Landing`] per staged mention that lands: a module constant `LIMIT` read in `size`; a
/// module constant `MAX` read as `cfg.MAX` through a parameter in `limits`; a class `Square` whose
/// base is `Shape`; a field `depth` of `Shape` read through a parameter `x` in `total`;
/// `Shape.side` read in `total` beside the field `side` of `Shape` and of `Other`; and
/// `Config.MAX` read in `limits` beside the module constant `MAX` and `Config`'s own `MAX`.
const LANDING_FILES: [Landing; 6] = [
    (
        "a module constant's read",
        "LIMIT = 3\n\n\ndef size():\n    return LIMIT\n",
        &["LIMIT", "size"],
        &[("size", "LIMIT", TIER_EXTRACTED)],
    ),
    (
        "a module constant's read as a parameter's attribute",
        "MAX = 1\n\n\ndef limits(cfg):\n    return cfg.MAX\n",
        &["MAX", "limits"],
        &[("limits", "MAX", TIER_EXTRACTED)],
    ),
    (
        "a base class",
        "class Shape:\n    pass\n\n\nclass Square(Shape):\n    pass\n",
        &["Shape", "Square"],
        &[("Square", "Shape", TIER_EXTRACTED)],
    ),
    (
        "an attribute read through a parameter",
        "class Shape:\n    depth = 2\n\n\ndef total(x):\n    return x.depth\n",
        &["Shape", "Shape.depth", "total"],
        &[("total", "Shape.depth", TIER_INFERRED)],
    ),
    (
        "an attribute read through its class",
        "class Shape:\n    side = 1\n\n\nclass Other:\n    side = 2\n\n\ndef total():\n    return Shape.side\n",
        &["Other", "Other.side", "Shape", "Shape.side", "total"],
        &[("total", "Shape.side", TIER_EXTRACTED)],
    ),
    (
        "a member constant's read through its class",
        "MAX = 1\n\n\nclass Config:\n    MAX = 3\n\n\ndef limits():\n    return Config.MAX\n",
        &["Config", "Config.MAX", "MAX", "limits"],
        &[("limits", "Config.MAX", TIER_EXTRACTED)],
    ),
];

/// GIVEN a project whose one Python file reads a constant, names a base class or reads an
/// attribute ([`LANDING_FILES`]),
/// WHEN the operator runs `rigger graph build`,
/// THEN the graph's `REFERENCES` and `CALLS` edges are exactly the row's, each a `REFERENCES` edge
/// and none a call: the `const` and `impl` mentions onto their one same-file definition,
/// `extracted`; the unqualified `field` mention onto the one field of its name, `inferred`, as a
/// field use never prefers its file; and the field mention qualified by `Shape` or `Config` onto
/// that class's field, `extracted`, where its leaf alone reaches two, `Config.MAX` never onto the
/// module constant `MAX`. Its code entities are the definitions alone, no placeholder standing for
/// any mention.
#[test]
fn graph_build_lands_a_python_constant_base_class_and_attribute_read_as_references() {
    use rigger::contextgraph::query::entity_id;

    let file = "f.py";
    let (found, expected): (Vec<_>, Vec<_>) = LANDING_FILES
        .into_iter()
        .map(|(what, source, definitions, edges)| {
            let (_project, graph) = common::cli::built_graph(&[(file, source)]);
            let found = (
                what,
                common::fixtures::landed(&graph),
                common::fixtures::live_file_and_entity_ids(&graph.whole().unwrap()),
            );
            let id = |path: &str| entity_id(file, path);
            let mut edges: Vec<_> = edges
                .iter()
                .map(|&(from, to, tier)| common::fixtures::reference(&id(from), &id(to), tier))
                .collect();
            edges.sort();
            let ids = definitions.iter().map(|&path| id(path)).collect();
            (found, (what, edges, [vec![file.to_string()], ids]))
        })
        .unzip();
    assert_eq!(
        found, expected,
        "each staged Python mention lands as one REFERENCES edge on the definition the resolver \
         reaches, no call and no placeholder"
    );
}
