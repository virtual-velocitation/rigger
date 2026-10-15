//! Periphery (integration) tests for spec 116 criterion 9, JS AND TS DEFINITIONS ARE NODES:
//! every JS and TS/TSX definition type is a definition of its kind. The crate's own tests pin
//! the rule on the symbols each composed query extracts; these run OUTSIDE the crate and guard
//! the boundary the rule crosses on its way to an operator: the graph `rigger graph build` folds
//! from each of the JS, TS and TSX definitions fixtures, where each definition is a node
//! carrying its kind string and line, a computed member and a `for .. of` assigning an existing
//! name are no node, and each reference hangs off the caller the fixture gives it.

mod common;

rigger::test_cases! {
    /// GIVEN a project whose one JS file is the JS definitions fixture,
    /// WHEN the operator runs `rigger graph build`,
    /// THEN the graph's code entities are exactly the JS lines' nodes (see [`JS_LINES_NODES`]),
    /// each carrying exactly its kind and line, and one placeholder per name a reference reaches
    /// that no definition gives, `run` among them; its `CALLS` edges are exactly the JS lines'
    /// (see [`JS_LINES_CALLS`]).
    #[cfg(feature = "symbols")]
    graph_build_makes_each_js_definition_a_node_of_its_kind_and_none_of_a_computed_member:
        assert_definitions_graph(
            ("f.js", common::fixtures::JS_DEFINITIONS_SOURCE),
            &[],
            &["run"],
            &[],
        );
    /// GIVEN a project whose one TS file is the TS definitions fixture,
    /// WHEN the operator runs `rigger graph build`,
    /// THEN its code entities are the JS lines' nodes, their line-15 type assertion a
    /// `constant`, and the TS lines' (see [`TS_LINES_NODES`]), the call `run()` of the JS lines
    /// landing on the placeholder `run` beside the abstract `run`, a declaration being no call
    /// target; its `CALLS` edges are the JS lines' and the TS lines' (see [`TS_LINES_CALLS`]).
    #[cfg(feature = "symbols")]
    graph_build_makes_each_ts_definition_a_node_of_its_kind:
        assert_definitions_graph(
            ("f.ts", common::fixtures::TS_DEFINITIONS_SOURCE),
            &TS_LINES_NODES,
            &TS_LINES_PLACEHOLDERS,
            &TS_LINES_CALLS,
        );
    /// GIVEN a project whose one TSX file is the TSX definitions fixture, the TS one with a JSX
    /// element in place of the type assertion,
    /// WHEN the operator runs `rigger graph build`,
    /// THEN its graph is the TS fixture's: the same nodes, kinds, lines and edges.
    #[cfg(feature = "symbols")]
    graph_build_makes_each_tsx_definition_a_node_of_its_kind:
        assert_definitions_graph(
            ("f.tsx", common::fixtures::TSX_DEFINITIONS_SOURCE),
            &TS_LINES_NODES,
            &TS_LINES_PLACEHOLDERS,
            &TS_LINES_CALLS,
        );
}

/// The nodes the JS lines (1-15) of every JS and TS definitions fixture fold to, as
/// `(id path, leaf, kind, line)` (spec 119, IDS FOLLOW THE LANGUAGE'S PATH):
///
///  - a declarator takes its value's kind: `named`, `gnamed` and the anonymous generator `g`
///    each a `function`, the anonymous class `Foo` and the named `Bar` each a `type`, a named
///    value of another leaf under its declarator (`named::fexp`, `gnamed::gexp`, `Bar::Baz`), a
///    member of an anonymous class value under the declarator (`Foo.s`), and every other value a
///    `constant`, the line-15 one and the valueless `lazy::y` included;
///  - a declarator named by a pattern makes each identifier it binds a `constant` (`a`, `c`,
///    `d`, `r`, `x`, `y`, `z`), the file-level `y` a node apart from `lazy::y`, and so does a
///    `for .. of` declaring one (`host::k`, `host::v`, `host::j`), while `for (q of m)` and
///    `for (o.p of m)` make no node;
///  - a method is a `method` under its class, private with its `#`, string-named between its
///    quotes and numeric as written, the computed one none, and `constructor` a `constructor`; a
///    class field is a `field` at the same leaves after a `.`, the computed one none, and an
///    arrow- or function-valued one (`Shape::act`, `Shape::cb`) a `method`.
#[cfg(feature = "symbols")]
const JS_LINES_NODES: [(&str, &str, &str, u64); 42] = [
    ("Bar", "Bar", "type", 7),
    ("Bar::Baz", "Baz", "type", 7),
    ("Foo", "Foo", "type", 6),
    ("Foo.s", "s", "field", 6),
    ("Shape", "Shape", "type", 11),
    ("Shape.#hid", "#hid", "field", 11),
    ("Shape.3", "3", "field", 11),
    ("Shape.field", "field", "field", 11),
    ("Shape.q-r", "q-r", "field", 11),
    ("Shape::#m", "#m", "method", 11),
    ("Shape::1", "1", "method", 11),
    ("Shape::a-b", "a-b", "method", 11),
    ("Shape::act", "act", "method", 11),
    ("Shape::area", "area", "method", 11),
    ("Shape::cb", "cb", "method", 11),
    ("Shape::constructor", "constructor", "constructor", 11),
    ("Shape::constructor::local", "local", "constant", 11),
    ("a", "a", "constant", 9),
    ("c", "c", "constant", 9),
    ("d", "d", "constant", 9),
    ("el", "el", "constant", 15),
    ("ex", "ex", "constant", 14),
    ("g", "g", "function", 5),
    ("gen", "gen", "function", 2),
    ("gnamed", "gnamed", "function", 4),
    ("gnamed::gexp", "gexp", "function", 4),
    ("handler", "handler", "function", 12),
    ("host", "host", "function", 1),
    ("host::j", "j", "constant", 1),
    ("host::k", "k", "constant", 1),
    ("host::loc", "loc", "constant", 1),
    ("host::v", "v", "constant", 1),
    ("lazy", "lazy", "function", 8),
    ("lazy::y", "y", "constant", 8),
    ("named", "named", "function", 3),
    ("named::fexp", "fexp", "function", 3),
    ("o2", "o2", "constant", 13),
    ("o2::p", "p", "function", 13),
    ("r", "r", "constant", 9),
    ("x", "x", "constant", 10),
    ("y", "y", "constant", 10),
    ("z", "z", "constant", 10),
];

/// The placeholders the JS lines' references reach in every fixture, each a name no definition
/// gives, the file-level `f` of `f()` included; `run` is a placeholder in the JS fixture alone.
#[cfg(feature = "symbols")]
const JS_LINES_PLACEHOLDERS: [&str; 8] =
    ["f", "init", "make", "paint", "probe", "spin", "tick", "use"];

/// The `CALLS` edges of the JS lines, `(caller path, callee leaf)`, each from the node of its
/// caller's path, `<file>::<path>` (spec 119 criterion 8): the calls in `host`'s local
/// const and loop from `host`, the two in the constructor's body and in its local const from
/// `Shape::constructor`, the generator value's from `g`, the static field initializer's from
/// `Foo.s`, the method's from `Shape::area` and the assigned function's from `handler`. `f()`, the initializer of a
/// pattern declarator, takes the file as caller and so is no edge.
#[cfg(feature = "symbols")]
const JS_LINES_CALLS: [(&str, &str); 8] = [
    ("Foo.s", "tick"),
    ("Shape::area", "paint"),
    ("Shape::constructor", "init"),
    ("Shape::constructor", "make"),
    ("g", "spin"),
    ("handler", "run"),
    ("host", "probe"),
    ("host", "use"),
];

/// The nodes the TS lines (16-23) of the TS and TSX definitions fixtures fold to, as
/// `(id path, leaf, kind, line)`, each member under its interface, class or enum: the ambient
/// `sig`, the interface's `m`, `s-m` and `4` and the abstract `run`, `s-a` and `5` each a
/// `declaration`; an interface, an abstract class, an enum and a type
/// alias each a `type`; an enum member a `variant` with or without a value, string-named between
/// its quotes and, with a value, numeric (`6`); a property signature a `field`, string-named,
/// numeric or named by a string alone, a class field a `field` and a function-valued one
/// (`act2`) a `method`; `namespace A.B` a `module` at `A.B` and `module M.N` one at `M.N`; and
/// `as const` a `constant`. `declare module "x"` is a module at `x`, the leaf of the
/// pattern-bound constant `x` of line 10, whose node the kind precedence settles a `constant`
/// at line 10, so it adds no row.
#[cfg(feature = "symbols")]
const TS_LINES_NODES: [(&str, &str, &str, u64); 26] = [
    ("A.B", "A.B", "module", 22),
    ("Abs", "Abs", "type", 18),
    ("Abs.fld", "fld", "field", 18),
    ("Abs.s-t", "s-t", "field", 18),
    ("Abs::5", "5", "declaration", 18),
    ("Abs::act2", "act2", "method", 18),
    ("Abs::run", "run", "declaration", 18),
    ("Abs::s-a", "s-a", "declaration", 18),
    ("Alias", "Alias", "type", 20),
    ("Color", "Color", "type", 19),
    ("Color::6", "6", "variant", 19),
    ("Color::Blue", "Blue", "variant", 19),
    ("Color::Dark-red", "Dark-red", "variant", 19),
    ("Color::Red", "Red", "variant", 19),
    ("Color::e-f", "e-f", "variant", 19),
    ("M.N", "M.N", "module", 24),
    ("Shape2", "Shape2", "type", 17),
    ("Shape2.2", "2", "field", 17),
    ("Shape2.bare", "bare", "field", 17),
    ("Shape2.p-q", "p-q", "field", 17),
    ("Shape2.prop", "prop", "field", 17),
    ("Shape2::4", "4", "declaration", 17),
    ("Shape2::m", "m", "declaration", 17),
    ("Shape2::s-m", "s-m", "declaration", 17),
    ("X", "X", "constant", 23),
    ("sig", "sig", "declaration", 16),
];

/// The placeholders the TS lines' references reach, each a type or call name no call target of
/// the file gives, the ambient function's file-level `Arg` included, and `run`, which the
/// JS lines call and the TS lines declare abstract.
#[cfg(feature = "symbols")]
const TS_LINES_PLACEHOLDERS: [&str; 11] = [
    "Arg", "FT", "PQ", "PType", "Ret", "Two", "V", "boot", "mk", "run", "setup",
];

/// The `CALLS` edges of the TS lines, `(caller path, callee leaf)`: each property signature's
/// and field's type reference from that member (`Shape2.p-q`, `Shape2.2`, `Shape2.prop`,
/// `Abs.fld`), the method and index signatures' from their interface, the enum member value's
/// call from `Color::Blue`, and the
/// module's and the namespace's body calls from `x` and `A.B`. The ambient function's types
/// take the file as caller and so are no edge.
#[cfg(feature = "symbols")]
const TS_LINES_CALLS: [(&str, &str); 9] = [
    ("A.B", "setup"),
    ("Abs.fld", "FT"),
    ("Color::Blue", "mk"),
    ("Shape2", "Ret"),
    ("Shape2", "V"),
    ("Shape2.2", "Two"),
    ("Shape2.p-q", "PQ"),
    ("Shape2.prop", "PType"),
    ("x", "boot"),
];

/// The graph `rigger graph build` folds from a project whose one file, `path`, is `source`, a JS
/// or TS definitions fixture, holds exactly the JS lines' nodes, placeholders and `CALLS` edges
/// and the `nodes`, `placeholders` and `calls` the fixture adds, each caller id the file and its
/// path, each callee the placeholder of its leaf, and each node of the language the extension
/// names.
#[cfg(feature = "symbols")]
fn assert_definitions_graph(
    (path, source): (&str, &str),
    nodes: &[(&str, &str, &str, u64)],
    placeholders: &[&str],
    calls: &[(&str, &str)],
) {
    use rigger::contextgraph::query::placeholder_id;

    let lang = if path.ends_with(".js") { "js" } else { "ts" };
    let mut edges: Vec<(String, String)> = JS_LINES_CALLS
        .iter()
        .chain(calls)
        .map(|(from, to)| (format!("{path}::{from}"), placeholder_id(path, to)))
        .collect();
    edges.sort();
    let edges: Vec<(&str, &str)> = edges
        .iter()
        .map(|(f, t)| (f.as_str(), t.as_str()))
        .collect();
    common::cli::assert_built_graph_holds(
        (path, lang, source),
        &[&JS_LINES_NODES[..], nodes].concat(),
        &[&JS_LINES_PLACEHOLDERS[..], placeholders].concat(),
        &edges,
        "every definition of the built graph at its path, none for a computed member or a for .. \
         of assigning an existing name, one placeholder per undefined name a reference reaches",
    );
}
