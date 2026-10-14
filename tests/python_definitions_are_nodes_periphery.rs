//! Periphery (integration) tests for spec 116 criterion 10, PYTHON DEFINITIONS ARE NODES: every
//! Python definition type is a definition of its kind. The crate's own tests pin the rule on the
//! symbols the registered Python query and the read-offs extract; these run OUTSIDE the crate and
//! guard the boundary the rule crosses on its way to an operator: the graph `rigger graph build`
//! folds from the Python definitions fixture, where each definition is a node carrying its kind
//! string and line, an assignment under `if TYPE_CHECKING:` and an attribute target are no node,
//! and each call hangs off the caller the fixture gives it.

mod common;

rigger::test_cases! {
    /// GIVEN a project whose one Python file is the Python definitions fixture,
    /// WHEN the operator runs `rigger graph build`,
    /// THEN the graph's code entities are exactly one node per leaf a definition gives, each
    /// carrying exactly its kind and line, and one placeholder per name a call reaches that no
    /// definition gives (`f`, `make`, `paint`, `probe`):
    ///
    ///  - a def is a `method` when its nearest enclosing definition is a class (`run` of a class
    ///    nested in a function, `area`, the decorated `deco`) and a `function` otherwise (`host`,
    ///    `inner` nested in a method); a class is a `type`;
    ///  - a module-level assignment makes a `constant` of each name it binds: one target of any
    ///    value (`z`, `u`, `m`, `n`), an annotated name with no value (`w`), each name of a tuple,
    ///    a parenthesized tuple or a list pattern, starred included (`a`, `b`, `j`, `q`, `c`, `d`),
    ///    and each target of a chain once (`x`, `y`);
    ///  - a class-body assignment makes a `field` of each name it binds (`h`, `e`, `g`, `k`);
    ///  - a type alias is a `type` at its name, generic (`X`) or not (`Y`);
    ///  - `t` under `if TYPE_CHECKING:`, the attribute target `self.x`, the augmented target `o`
    ///    and the type parameter `T` are no node.
    ///
    /// Its `CALLS` edges are exactly: `probe()` from `host`, `make()` from `Shape` and `paint()`
    /// from `inner`. The `f()` of the list pattern and of the chain take the file as caller and so
    /// are no edge.
    #[cfg(feature = "symbols")]
    graph_build_makes_each_python_definition_a_node_of_its_kind_and_none_under_type_checking:
        common::cli::assert_built_graph_holds(
            ("f.py", "python", common::fixtures::PYTHON_DEFINITIONS_SOURCE),
            &[
                ("Local", "type", 3),
                ("Shape", "type", 6),
                ("X", "type", 24),
                ("Y", "type", 29),
                ("a", "constant", 17),
                ("area", "method", 10),
                ("b", "constant", 17),
                ("c", "constant", 18),
                ("d", "constant", 18),
                ("deco", "method", 15),
                ("e", "field", 8),
                ("g", "field", 8),
                ("h", "field", 7),
                ("host", "function", 1),
                ("inner", "function", 12),
                ("j", "constant", 28),
                ("k", "field", 9),
                ("m", "constant", 26),
                ("n", "constant", 27),
                ("q", "constant", 28),
                ("run", "method", 4),
                ("u", "constant", 25),
                ("w", "constant", 21),
                ("x", "constant", 19),
                ("y", "constant", 19),
                ("z", "constant", 20),
            ],
            &["f", "make", "paint", "probe"],
            &[
                ("f.py::Shape", "f.py::make"),
                ("f.py::host", "f.py::probe"),
                ("f.py::inner", "f.py::paint"),
            ],
            "every code entity of the built graph: one per definition leaf, none under \
             if TYPE_CHECKING: or for an attribute, augmented or type-parameter name, one \
             placeholder per undefined name a call reaches",
        );
}
