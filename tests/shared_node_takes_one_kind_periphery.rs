//! Periphery (contract and integration) tests for spec 116 criterion 5, A SHARED NODE TAKES ONE
//! KIND: the definitions one file gives one path fold onto one node (spec 119, IDS FOLLOW THE
//! LANGUAGE'S PATH), which takes the kind and line of the one whose kind comes first in
//! `CODE_KIND_PRECEDENCE`. The graph crate's own tests
//! pin the fold over hand-built keyed events; these run OUTSIDE the crate and guard the two
//! boundaries the rule crosses: the list every crate reads the order from, and the graph
//! `rigger graph build` folds from what the extractor emits for a real source file.

mod common;

/// `CODE_KIND_PRECEDENCE` is the order the spec states, kind string by kind string: sixteen
/// kinds, a type first and `other` last, each spelled as the log and a folded graph carry it.
#[test]
fn the_precedence_list_is_the_spec_order_of_the_kind_strings_a_graph_holds() {
    assert_eq!(
        rigger::contextgraph::CODE_KIND_PRECEDENCE,
        [
            "type",
            "trait",
            "impl",
            "function",
            "method",
            "macro",
            "constructor",
            "variant",
            "constant",
            "static",
            "variable",
            "declaration",
            "property",
            "field",
            "module",
            "other",
        ]
    );
}

/// A Rust file giving four paths two definitions each: a `macro_rules!` above a fn of its leaf
/// (`x`) and a fn above a `macro_rules!` of its leaf (`y`), a fn above a type of its leaf
/// (`Maker`) and a type above a fn of its leaf (`Kept`); and two `fn new` in two impls, at two
/// paths (`A::new`, `B::new`; spec 119, IDS FOLLOW THE LANGUAGE'S PATH).
#[cfg(feature = "symbols")]
const SOURCE: &str = "\
pub struct A;
pub struct B;
impl A {
    pub fn new() -> Self {
        A
    }
}
impl B {
    pub fn new() -> Self {
        B
    }
}
macro_rules! x {
    () => {};
}
pub fn x() {}
pub fn y() {}
macro_rules! y {
    () => {};
}
pub fn Maker() {}
pub struct Maker;
pub struct Kept;
pub fn Kept() {}
";

/// `graph` holds exactly [`SOURCE`]'s ten nodes for `f.rs`, one per path, each carrying exactly
/// its leaf and its winner's kind and line in a file holding `above` lines over the source:
/// `A::new` and `B::new` each its own method, `x` and `y` the function at the fn's line
/// whichever side the macro sits on (the extractor gives a `macro_rules!` a kind listed after
/// `function`), `Maker` and `Kept` the type at the type's line whichever side the fn sits on.
#[cfg(feature = "symbols")]
fn every_path_is_its_precedence_winner(
    graph: &rigger::contextgraph::sqlite::Projector,
    above: u64,
    why: &str,
) {
    use common::fixtures::{attrs_of, definition, live_node_ids};
    use rigger::contextgraph::{query::entity_id, KIND_CODE_ENTITY};

    let settled = [
        ("A", "A", "type", 1),
        ("A::new", "new", "method", 4),
        ("B", "B", "type", 2),
        ("B::new", "new", "method", 9),
        ("Kept", "Kept", "type", 23),
        ("Maker", "Maker", "type", 22),
        ("impl A", "impl A", "impl", 3),
        ("impl B", "impl B", "impl", 8),
        ("x", "x", "function", 16),
        ("y", "y", "function", 17),
    ];
    let ids = settled.map(|(path, _, _, _)| entity_id("f.rs", path));
    assert_eq!(
        live_node_ids(&graph.whole().unwrap(), KIND_CODE_ENTITY),
        ids.to_vec(),
        "every code entity of the built graph, one node per path: {why}"
    );
    assert_eq!(
        ids.map(|id| attrs_of(graph, &id)),
        settled.map(|(_, name, kind, line)| definition(name, kind, line + above)),
        "each node carries its leaf and its winner's kind and line, and nothing else: {why}"
    );
}

/// GIVEN a project whose one Rust file is [`SOURCE`],
/// WHEN the operator runs `rigger graph build`,
/// THEN each path is one node carrying exactly its winner's kind and line; no path takes the
/// definition the build happened to fold last.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_settles_a_path_one_file_defines_twice_on_its_precedence_winner() {
    let (_project, graph) = common::cli::built_graph(&[("f.rs", SOURCE)]);
    every_path_is_its_precedence_winner(&graph, 0, "the first build");
}

/// GIVEN a project whose graph is built over [`SOURCE`],
/// WHEN the operator puts two comment lines above the source and runs `rigger graph build` again,
/// THEN each path is still one node carrying its winner's kind, at the winner's new line, two
/// lower: the definitions the file's earlier generation gave a node are no candidates beside the
/// new generation's, so no node keeps a line the file no longer holds a definition at.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_again_over_shifted_lines_moves_each_shared_node_to_its_winners_new_line() {
    let (project, graph) = common::cli::built_graph(&[("f.rs", SOURCE)]);
    drop(graph);
    let moved = format!("// moved\n// down\n{SOURCE}");
    every_path_is_its_precedence_winner(
        &common::cli::rebuilt_graph(project.path(), &[("f.rs", &moved)]),
        2,
        "the build after every line moved two down",
    );
}
