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
    /// now reaching the abstract `run` rather than a placeholder; its `CALLS` edges are the JS
    /// lines' and the TS lines' (see [`TS_LINES_CALLS`]).
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
/// `(leaf, kind, line)`:
///
///  - a declarator takes its value's kind: `named`, `gnamed` and the anonymous generator `g`
///    each a `function`, the anonymous class `Foo` and the named `Bar` each a `type` beside the
///    named class `Baz`, and every other value a `constant`, the line-15 one and the valueless
///    `y` included;
///  - a declarator named by a pattern makes each identifier it binds a `constant` (`a`, `c`,
///    `d`, `r`, `x`, `y`, `z`), the second `y` sharing the first's node, and so does a `for .. of`
///    declaring one (`k`, `v`, `j`), while `for (q of m)` and `for (o.p of m)` make no node;
///  - a method is a `method` at its leaf, private with its `#`, string-named between its quotes
///    and numeric as written, the computed one none, and `constructor` a `constructor`; a class
///    field is a `field` at the same leaves, the computed one none, and an arrow- or
///    function-valued one (`act`, `cb`) a `method`.
#[cfg(feature = "symbols")]
const JS_LINES_NODES: [(&str, &str, u64); 41] = [
    ("#hid", "field", 11),
    ("#m", "method", 11),
    ("1", "method", 11),
    ("3", "field", 11),
    ("Bar", "type", 7),
    ("Baz", "type", 7),
    ("Foo", "type", 6),
    ("Shape", "type", 11),
    ("a", "constant", 9),
    ("a-b", "method", 11),
    ("act", "method", 11),
    ("area", "method", 11),
    ("c", "constant", 9),
    ("cb", "method", 11),
    ("constructor", "constructor", 11),
    ("d", "constant", 9),
    ("el", "constant", 15),
    ("ex", "constant", 14),
    ("fexp", "function", 3),
    ("field", "field", 11),
    ("g", "function", 5),
    ("gen", "function", 2),
    ("gexp", "function", 4),
    ("gnamed", "function", 4),
    ("handler", "function", 12),
    ("host", "function", 1),
    ("j", "constant", 1),
    ("k", "constant", 1),
    ("lazy", "function", 8),
    ("loc", "constant", 1),
    ("local", "constant", 11),
    ("named", "function", 3),
    ("o2", "constant", 13),
    ("p", "function", 13),
    ("q-r", "field", 11),
    ("r", "constant", 9),
    ("s", "field", 6),
    ("v", "constant", 1),
    ("x", "constant", 10),
    ("y", "constant", 8),
    ("z", "constant", 10),
];

/// The placeholders the JS lines' references reach in every fixture, each a name no definition
/// gives, the file-level `f` of `f()` included; `run` is a placeholder in the JS fixture alone.
#[cfg(feature = "symbols")]
const JS_LINES_PLACEHOLDERS: [&str; 8] =
    ["f", "init", "make", "paint", "probe", "spin", "tick", "use"];

/// The `CALLS` edges of the JS lines, `(caller leaf, callee leaf)`: the calls in `host`'s local
/// const and loop from `host`, the two in the constructor's body and in its local const from
/// `constructor`, the generator value's from `g`, the static field initializer's from `s`, the
/// method's from `area` and the assigned function's from `handler`. `f()`, the initializer of a
/// pattern declarator, takes the file as caller and so is no edge.
#[cfg(feature = "symbols")]
const JS_LINES_CALLS: [(&str, &str); 8] = [
    ("area", "paint"),
    ("constructor", "init"),
    ("constructor", "make"),
    ("g", "spin"),
    ("handler", "run"),
    ("host", "probe"),
    ("host", "use"),
    ("s", "tick"),
];

/// The nodes the TS lines (16-23) of the TS and TSX definitions fixtures fold to, as
/// `(leaf, kind, line)`: the ambient `sig`, the interface's `m`, `s-m` and `4` and the abstract
/// `run`, `s-a` and `5` each a `declaration`; an interface, an abstract class, an enum and a type
/// alias each a `type`; an enum member a `variant` with or without a value, string-named between
/// its quotes and, with a value, numeric (`6`); a property signature a `field`, string-named,
/// numeric or named by a string alone, a class field a `field` and a function-valued one
/// (`act2`) a `method`; `namespace A.B` a `module` at `A.B` and `module M.N` one at `M.N`; and
/// `as const` a `constant`. `declare module "x"` is a module at `x`, the leaf of the
/// pattern-bound constant `x` of line 10, whose node the kind precedence settles a `constant`
/// at line 10, so it adds no row.
#[cfg(feature = "symbols")]
const TS_LINES_NODES: [(&str, &str, u64); 26] = [
    ("2", "field", 17),
    ("4", "declaration", 17),
    ("5", "declaration", 18),
    ("6", "variant", 19),
    ("A.B", "module", 22),
    ("Abs", "type", 18),
    ("Alias", "type", 20),
    ("Blue", "variant", 19),
    ("Color", "type", 19),
    ("Dark-red", "variant", 19),
    ("M.N", "module", 24),
    ("Red", "variant", 19),
    ("Shape2", "type", 17),
    ("X", "constant", 23),
    ("act2", "method", 18),
    ("bare", "field", 17),
    ("e-f", "variant", 19),
    ("fld", "field", 18),
    ("m", "declaration", 17),
    ("p-q", "field", 17),
    ("prop", "field", 17),
    ("run", "declaration", 18),
    ("s-a", "declaration", 18),
    ("s-m", "declaration", 17),
    ("s-t", "field", 18),
    ("sig", "declaration", 16),
];

/// The placeholders the TS lines' references reach, each a type or call name no definition
/// gives, the ambient function's file-level `Arg` included.
#[cfg(feature = "symbols")]
const TS_LINES_PLACEHOLDERS: [&str; 10] = [
    "Arg", "FT", "PQ", "PType", "Ret", "Two", "V", "boot", "mk", "setup",
];

/// The `CALLS` edges of the TS lines, `(caller leaf, callee leaf)`: each property signature's
/// and field's type reference from that member (`p-q`, `2`, `prop`, `fld`), the method and
/// index signatures' from their interface, the enum member value's call from `Blue`, and the
/// module's and the namespace's body calls from `x` and `A.B`. The ambient function's types
/// take the file as caller and so are no edge.
#[cfg(feature = "symbols")]
const TS_LINES_CALLS: [(&str, &str); 9] = [
    ("2", "Two"),
    ("A.B", "setup"),
    ("Blue", "mk"),
    ("Shape2", "Ret"),
    ("Shape2", "V"),
    ("fld", "FT"),
    ("p-q", "PQ"),
    ("prop", "PType"),
    ("x", "boot"),
];

/// The graph `rigger graph build` folds from a project whose one file, `path`, is `source`, a JS
/// or TS definitions fixture, holds exactly the JS lines' nodes, placeholders and `CALLS` edges
/// and the `nodes`, `placeholders` and `calls` the fixture adds, each id the file and its leaf
/// and each node of the language the extension names.
#[cfg(feature = "symbols")]
fn assert_definitions_graph(
    (path, source): (&str, &str),
    nodes: &[(&str, &str, u64)],
    placeholders: &[&str],
    calls: &[(&str, &str)],
) {
    let lang = if path.ends_with(".js") { "js" } else { "ts" };
    let mut edges: Vec<(String, String)> = JS_LINES_CALLS
        .iter()
        .chain(calls)
        .map(|(from, to)| (format!("{path}::{from}"), format!("{path}::{to}")))
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
        "every code entity of the built graph: one per definition leaf, none for a computed \
         member or a for .. of assigning an existing name, one placeholder per undefined name a \
         reference reaches",
    );
}
