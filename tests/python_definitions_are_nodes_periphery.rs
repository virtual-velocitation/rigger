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
    /// THEN the graph's definitions are exactly one node per definition, at its `<file>::<path>` id
    /// (spec 119, IDS FOLLOW THE LANGUAGE'S PATH), each carrying exactly its leaf, kind and line,
    /// and its placeholders one per name a call reaches that no definition gives (`f`, `make`,
    /// `paint`, `probe`):
    ///
    ///  - a def is a `method` when its nearest enclosing definition is a class
    ///    (`host::Local::run` of a class nested in a function, `Shape::area`, the decorated
    ///    `Shape::deco`) and a `function` otherwise (`host`, `Shape::area::inner` nested in a
    ///    method); a class is a `type`;
    ///  - a module-level assignment makes a `constant` of each name it binds: one target of any
    ///    value (`z`, `u`, `m`, `n`), an annotated name with no value (`w`), each name of a tuple,
    ///    a parenthesized tuple or a list pattern, starred included (`a`, `b`, `j`, `q`, `c`, `d`),
    ///    and each target of a chain once (`x`, `y`);
    ///  - a class-body assignment makes a `field` of each name it binds (`Shape.h`, `Shape.e`,
    ///    `Shape.g`, `Shape.k`);
    ///  - a type alias is a `type` at its name, generic (`X`) or not (`Y`);
    ///  - `t` under `if TYPE_CHECKING:`, the attribute target `self.x`, the augmented target `o`
    ///    and the type parameter `T` are no node.
    ///
    /// Its `CALLS` edges are exactly: `probe()` from `host`, `make()` from `Shape.k`, the
    /// class-body assignment of one target spanning its whole value, and `paint()` from
    /// `Shape::area::inner`, each from the node of its caller's path, `<file>::<path>` (spec 119
    /// criterion 8). The `f()` of the list pattern and of the
    /// chain take the file as caller and so are no edge.
    #[cfg(feature = "symbols")]
    graph_build_makes_each_python_definition_a_node_of_its_kind_and_none_under_type_checking:
        common::cli::assert_built_graph_holds(
            ("f.py", "python", common::fixtures::PYTHON_DEFINITIONS_SOURCE),
            &[
                ("Shape", "Shape", "type", 6),
                ("Shape.e", "e", "field", 8),
                ("Shape.g", "g", "field", 8),
                ("Shape.h", "h", "field", 7),
                ("Shape.k", "k", "field", 9),
                ("Shape::area", "area", "method", 10),
                ("Shape::area::inner", "inner", "function", 12),
                ("Shape::deco", "deco", "method", 15),
                ("X", "X", "type", 24),
                ("Y", "Y", "type", 29),
                ("a", "a", "constant", 17),
                ("b", "b", "constant", 17),
                ("c", "c", "constant", 18),
                ("d", "d", "constant", 18),
                ("host", "host", "function", 1),
                ("host::Local", "Local", "type", 3),
                ("host::Local::run", "run", "method", 4),
                ("j", "j", "constant", 28),
                ("m", "m", "constant", 26),
                ("n", "n", "constant", 27),
                ("q", "q", "constant", 28),
                ("u", "u", "constant", 25),
                ("w", "w", "constant", 21),
                ("x", "x", "constant", 19),
                ("y", "y", "constant", 19),
                ("z", "z", "constant", 20),
            ],
            &["f", "make", "paint", "probe"],
            &[
                ("f.py::Shape.k", "f.py::%make"),
                ("f.py::Shape::area::inner", "f.py::%paint"),
                ("f.py::host", "f.py::%probe"),
            ],
            "every definition of the built graph at its path, none under if TYPE_CHECKING: or for \
             an attribute, augmented or type-parameter name, one placeholder per undefined name a \
             call reaches",
        );
}
