//! Periphery (integration) tests for spec 119 criterion 10, ONE PATH IS ONE EVENT: the extraction
//! emits one `CodeEntityExtracted` per path, its winner the first by `CODE_KIND_PRECEDENCE`, then
//! the lower line. The emit pass's own tests pin the events; these run OUTSIDE the crate and guard
//! the seam the events cross, the graph `rigger graph build` folds from them for a real source
//! file: a path's one node and its one `CONTAINS`, from the winner's parent alone, since a twin
//! that loses emits no event to fold a `CONTAINS` from its own parent.

mod common;

#[cfg(feature = "symbols")]
use common::fixtures::{assert_live, attrs_of, definition, definition_in};
#[cfg(feature = "symbols")]
use rigger::contextgraph::REL_CONTAINS;

/// GIVEN a project whose one TS file holds a `namespace A.B` above a `class A` whose field is `B`,
/// WHEN the operator runs `rigger graph build`,
/// THEN the path `A.B` is one node carrying the field's kind and line, and its one `CONTAINS`
/// comes from the field's class: the namespace, which would hang it off the file, emits nothing.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_folds_a_namespace_beside_a_field_of_its_path_as_the_field_its_class_contains() {
    let (_project, graph) = common::cli::built_graph(&[(
        "c.ts",
        common::fixtures::NAMESPACE_A_B_BESIDE_CLASS_A_FIELD_B,
    )]);
    assert_eq!(
        attrs_of(&graph, "c.ts::A.B"),
        definition_in("ts", "B", "field", 3),
        "the path's one node is the field's"
    );
    assert_live(
        &graph,
        REL_CONTAINS,
        &[("c.ts", "c.ts::A"), ("c.ts::A", "c.ts::A.B")],
        "one CONTAINS into A.B, from the field's class",
    );
}

/// The node the method `c` of `a::B` folds onto in `a.rs`.
#[cfg(feature = "symbols")]
const METHOD_C: &str = "a.rs::a::B::c";

/// The node the file-level `impl a::B` of `a.rs` folds onto.
#[cfg(feature = "symbols")]
const FILE_LEVEL_IMPL: &str = "a.rs::impl a::B";

/// The `CONTAINS` edges both generations of `a.rs` fold beside the ones `c` and the `impl B`
/// inside `mod a` take: the file holding `mod a` and the file-level impl, and `mod a` its `B`.
#[cfg(feature = "symbols")]
const AROUND_METHOD_C: [(&str, &str); 3] = [
    ("a.rs", "a.rs::a"),
    ("a.rs", FILE_LEVEL_IMPL),
    ("a.rs::a", "a.rs::a::B"),
];

/// GIVEN a project whose one Rust file defines the method `a::B::c` in the `impl B` inside
/// `mod a` and again, further down the file, in a file-level `impl a::B`,
/// WHEN the operator runs `rigger graph build`,
/// THEN the path is one node at the lower line's twin, its one `CONTAINS` from that twin's
/// impl; and WHEN the operator blanks out the inner impl and builds again,
/// THEN the node takes the other twin's line and its one `CONTAINS` comes from the other impl.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_again_without_the_winning_twin_hands_its_path_to_the_other() {
    use common::fixtures::{METHOD_C_IN_THE_FILE_LEVEL_IMPL, METHOD_C_IN_TWO_IMPLS};

    let (project, graph) = common::cli::built_graph(&[("a.rs", METHOD_C_IN_TWO_IMPLS)]);
    assert_eq!(
        attrs_of(&graph, METHOD_C),
        definition("c", "method", 4),
        "both twins: the lower line's node"
    );
    assert_live(
        &graph,
        REL_CONTAINS,
        &[
            &AROUND_METHOD_C[..],
            &[
                ("a.rs::a", "a.rs::a::impl B"),
                ("a.rs::a::impl B", METHOD_C),
            ],
        ]
        .concat(),
        "both twins: one CONTAINS into a::B::c, from the lower line's impl",
    );
    drop(graph);

    let graph =
        common::cli::rebuilt_graph(project.path(), &[("a.rs", METHOD_C_IN_THE_FILE_LEVEL_IMPL)]);
    assert_eq!(
        attrs_of(&graph, METHOD_C),
        definition("c", "method", 8),
        "the winner removed: the other twin's line"
    );
    assert_live(
        &graph,
        REL_CONTAINS,
        &[&AROUND_METHOD_C[..], &[(FILE_LEVEL_IMPL, METHOD_C)]].concat(),
        "the winner removed: the one CONTAINS from the other twin's impl",
    );
}
