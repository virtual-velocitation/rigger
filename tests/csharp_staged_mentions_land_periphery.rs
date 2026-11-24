//! Periphery (integration) tests for spec 117, C# STAGES ITS ROW, across the seam the grounder
//! crate's `tests/facts_csharp.rs` stops short of: that suite pins the mentions the bundled C#
//! grammar stages; this one pins that the real `rigger graph build` stores an `impl`, a `type` and
//! a `field` mention in the code store with their kinds, the one place the `impl` kind shows apart
//! from a `type`, and lands each in the graph through the resolver as one `REFERENCES` edge and
//! never a call:
//!
//!  - a base list's `IShape` lands on that interface, and a parameter's `List<IShape>` on `List`
//!    and on `IShape`, each `extracted` onto its one same-file definition;
//!  - `Square.SIDES` lands on `Square`'s constant `SIDES`, `extracted`, through its qualifier,
//!    where a field mention by its leaf alone never reaches a constant; and `s.Side`, its object
//!    lowercase and so no qualifier, lands on the one field `Side`, `inferred` though that field
//!    is in the same file.
//!
//! The `///` block's span reaches the code store in `doc_spans_reach_the_code_store_periphery`.
//!
//! Every test extracts real source, so the file runs on the `symbols` lane alone.

use crate::common;
use crate::symbol_index::mention_in;

/// A C# file holding the criterion's row beside the definitions its mentions name: an interface
/// `IShape` (line 1), a class `List<T>` (line 2), and under a two-line `///` block a class
/// `Square : IShape` (lines 5-9) with a constant `SIDES`, a field `Side` and a method
/// `Draw(List<IShape> s)` reading `Square.SIDES` and assigning `s.Side`.
const SQUARE_SOURCE: &str = "interface IShape { }\nclass List<T> { }\n/// <summary>A square.</summary>\n/// <remarks>Its <c>Side</c> is fixed.</remarks>\nclass Square : IShape {\n    const int SIDES = 4;\n    int Side;\n    void Draw(List<IShape> s) { var n = Square.SIDES; s.Side = 2; }\n}\n";

/// GIVEN a project whose one C# file ([`SQUARE_SOURCE`]) defines `IShape`, `List<T>` and a class
/// `Square : IShape` whose method `Draw(List<IShape> s)` reads `Square.SIDES` and assigns
/// `s.Side`,
/// WHEN the operator runs `rigger graph build`,
/// THEN the code store holds the file's five mentions exactly: `Square`'s base `IShape` an `impl`,
/// `Draw`'s `IShape` and `List` each a `type`, and its `Square.SIDES`, of leaf `SIDES`, and `Side`
/// each a `field`; and the graph's `REFERENCES` and `CALLS` edges are exactly one `REFERENCES`
/// edge each, none a call: from `Square` onto `IShape`, its base; from `Square::Draw` onto `List`
/// and onto `IShape`, its parameter's type and that type's argument, each `extracted`; from
/// `Square::Draw` onto the constant `Square::SIDES`, `extracted`, the qualifier `Square` keeping
/// it; and from `Square::Draw` onto the field `Square.Side`, `inferred`, as an unqualified field
/// use never prefers its file. Its code entities are the definitions alone, no placeholder
/// standing for any mention.
#[test]
fn graph_build_lands_a_csharp_base_type_qualified_member_and_field_as_references() {
    use rigger::contextgraph::query::entity_id;
    use rigger::contextgraph::{
        REF_KIND_FIELD, REF_KIND_IMPL, REF_KIND_TYPE, TIER_EXTRACTED, TIER_INFERRED,
    };
    use rigger_domain::code_view::CodeView;

    let file = "f.cs";
    let (_project, graph) = common::cli::built_graph(&[(file, SQUARE_SOURCE)]);

    // A mention inside the definition at index `at` (`Square` is 2, `Draw` 5), named by its leaf.
    let leaf_in = |at, leaf, kind, line| mention_in(Some(at), leaf, leaf, Some(kind), line, false);
    let mentions = vec![
        leaf_in(2, "IShape", REF_KIND_IMPL, 5),
        leaf_in(5, "IShape", REF_KIND_TYPE, 8),
        leaf_in(5, "List", REF_KIND_TYPE, 8),
        mention_in(
            Some(5),
            "Square.SIDES",
            "SIDES",
            Some(REF_KIND_FIELD),
            8,
            false,
        ),
        leaf_in(5, "Side", REF_KIND_FIELD, 8),
    ];
    let id = |path| entity_id(file, path);
    let mut edges = vec![
        common::fixtures::reference(&id("Square"), &id("IShape"), TIER_EXTRACTED),
        common::fixtures::reference(&id("Square::Draw"), &id("IShape"), TIER_EXTRACTED),
        common::fixtures::reference(&id("Square::Draw"), &id("List"), TIER_EXTRACTED),
        common::fixtures::reference(&id("Square::Draw"), &id("Square::SIDES"), TIER_EXTRACTED),
        common::fixtures::reference(&id("Square::Draw"), &id("Square.Side"), TIER_INFERRED),
    ];
    edges.sort();
    let entities = [
        "IShape",
        "List",
        "Square",
        "Square.Side",
        "Square::Draw",
        "Square::SIDES",
    ]
    .map(id)
    .to_vec();
    assert_eq!(
        (
            graph
                .view
                .file(file)
                .map(|facts| facts.into_owned().mentions),
            common::fixtures::landed(&graph),
            common::fixtures::live_file_and_entity_ids(&graph.whole().unwrap()),
        ),
        (Some(mentions), edges, [vec![file.to_string()], entities]),
        "the store holds the base's impl mention, the parameter's two type mentions and the two \
         field mentions, `Square.SIDES` named as written; each lands as one REFERENCES edge, no \
         call; and no placeholder stands for any mention"
    );
}
