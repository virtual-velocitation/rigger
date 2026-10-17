//! Periphery (integration) test for spec 116 criterion 3, THE NAME FILTER DROPS A NAMELESS
//! DEFINITION: `extract` keeps no definition whose leaf is empty, nor one whose leaf is `_` in Go
//! or Rust. The crate's own tests pin the filter on the extracted symbols; this runs OUTSIDE the
//! crate and guards the boundary the filter crosses on its way to an operator, the graph
//! `rigger graph build` folds: a dropped definition is no node there and no caller of the calls
//! in its body, and a `_` the filter keeps is a node.

mod common;

/// GIVEN a project holding the three name filter sources: a Go file with a function, a method
/// and a type each named `_` beside a named type and function, a JS file binding a const named
/// `_` and a Python file binding a module-level `_`,
/// WHEN the operator runs `rigger graph build`,
/// THEN the graph's code entities are exactly the Go file's named type and function, the JS and
/// the Python `_`, and one placeholder per name a Go reference reaches that no definition gives
/// (`int`, `other`, `probe`, `reach`); no entity is `main.go::_`. Its `CALLS` edges are exactly the named
/// function's call: the calls inside the Go `func _()` and `func (T) _()` have the file as
/// caller and make no edge.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_makes_no_node_of_a_go_blank_definition_and_keeps_a_js_and_python_underscore() {
    use rigger::contextgraph::KIND_CODE_ENTITY;

    let (_project, graph) = common::cli::built_graph(&[
        ("main.go", common::fixtures::GO_BLANK_SOURCE),
        ("a.js", common::fixtures::JS_UNDERSCORE_SOURCE),
        ("b.py", common::fixtures::PY_UNDERSCORE_SOURCE),
    ]);

    let entities: Vec<String> = [
        "a.js::_",
        "b.py::_",
        "main.go::%int",
        "main.go::%other",
        "main.go::%probe",
        "main.go::%reach",
        "main.go::T",
        "main.go::named",
    ]
    .map(str::to_string)
    .to_vec();
    assert_eq!(
        common::fixtures::live_node_ids(&graph.whole().unwrap(), KIND_CODE_ENTITY),
        entities,
        "every code entity of the built graph: none for a Go `_`, one for a JS and a Python `_`"
    );
    let calls: Vec<(String, String)> = vec![("main.go::named".into(), "main.go::%other".into())];
    assert_eq!(
        common::fixtures::live_calls(&graph),
        calls,
        "every CALLS edge of the built graph: none from a dropped definition"
    );
}
