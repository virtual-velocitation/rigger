//! Periphery (contract and integration) tests for spec 116 criterion 5, A SHARED NODE TAKES ONE
//! KIND: the definitions one file gives one leaf fold onto one node, which takes the kind and
//! line of the one whose kind comes first in `CODE_KIND_PRECEDENCE`. The graph crate's own tests
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

/// GIVEN a project whose one Rust file gives five leaves two definitions each: two `fn new` in
/// two impls, a `macro_rules!` above a fn of its leaf (`x`) and a fn above a `macro_rules!` of its
/// leaf (`y`), a fn above a type of its leaf (`Maker`) and a type above a fn of its leaf (`Kept`),
/// WHEN the operator runs `rigger graph build`,
/// THEN each leaf is one node carrying exactly its winner's kind and line: `new` the method at
/// the lower line, `x` and `y` the function at the fn's line whichever side the macro sits on
/// (the extractor gives a `macro_rules!` a kind listed after `function`),
/// `Maker` and `Kept` the type at the type's line whichever side the fn sits on. No leaf takes
/// the definition the build happened to fold last.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_settles_a_leaf_one_file_defines_twice_on_its_precedence_winner() {
    use common::fixtures::{attrs_of, definition, live_node_ids};
    use rigger::contextgraph::KIND_CODE_ENTITY;

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
    let (_project, graph) = common::cli::built_graph(&[("f.rs", SOURCE)]);

    let settled = [
        ("A", "type", 1),
        ("B", "type", 2),
        ("Kept", "type", 23),
        ("Maker", "type", 22),
        ("impl A", "impl", 3),
        ("impl B", "impl", 8),
        ("new", "method", 4),
        ("x", "function", 16),
        ("y", "function", 17),
    ];
    assert_eq!(
        live_node_ids(&graph.whole().unwrap(), KIND_CODE_ENTITY),
        settled.map(|(name, _, _)| format!("f.rs::{name}")).to_vec(),
        "every code entity of the built graph: one node per leaf, however many definitions"
    );
    assert_eq!(
        settled.map(|(name, _, _)| attrs_of(&graph, &format!("f.rs::{name}"))),
        settled.map(|(name, kind, line)| definition(name, kind, line)),
        "each node carries its winner's kind and line, and nothing else"
    );
}
