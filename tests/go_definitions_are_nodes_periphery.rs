//! Periphery (contract and integration) tests for spec 116 criterion 7, GO DEFINITIONS ARE
//! NODES: every Go definition type is a definition of its kind. The crate's own tests pin the
//! rule on the extracted symbols; these run OUTSIDE the crate and guard the boundary the
//! rule crosses on its way to an operator:
//!
//!  - the graph `rigger graph build` folds from the Go definitions fixture: each definition is a
//!    node carrying its kind string and line, a function-local `var` and an alias named `_` are
//!    no node, and each call hangs off the caller the fixture gives it.

#[cfg(feature = "symbols")]
use crate::common;

rigger::test_cases! {
    /// GIVEN a project whose one Go file is the Go definitions fixture,
    /// WHEN the operator runs `rigger graph build`,
    /// THEN the graph's definitions are exactly one node per definition, at its `<file>::<path>` id
    /// (spec 119, IDS FOLLOW THE LANGUAGE'S PATH), each carrying exactly its leaf, kind and line,
    /// and its placeholders one per name a call reaches that no call target of the file gives (`f`,
    /// `g`, `h`, `k`, `probe`); a reference to a type, an embedded field's, a parameter's, a
    /// receiver's or an alias's, is a type reference, no call, and makes neither a placeholder nor
    /// a `CALLS` edge (spec 117, CAPTURES):
    ///
    ///  - each name of a const spec is a `constant`, the two of `const a, b` and the function-local
    ///    `host::inner` included; a package-level `var` is a `variable`, single (`single`), grouped
    ///    (`grouped`) or naming two (`m`, `n`), while the function-local `local` and grouped `deep`
    ///    are no node;
    ///  - a named field is a `field` under its struct, each name of the field declaration naming
    ///    two included (`Shape.x`, `Shape.y`), and an embedded field a `field` at its type's base
    ///    identifier, the pointer of `Ptr`, the package of `Qual` and the type argument of `Gen`
    ///    stripped;
    ///  - the embedded field `Shape.Base` is a node apart from the struct type `Base` it names;
    ///  - the alias `Alias` is a `type`, and the alias named `_` no definition;
    ///  - the interface's method `Drawer::Draw`, the function `ext` and the method `Shape::bare`,
    ///    none with a body, are each a `declaration`, and the method `Shape::area` a `method`, each
    ///    method under its receiver's type.
    ///
    /// Its `CALLS` edges are exactly: both calls of `const a, b = f(), g()` from `a`, both of
    /// `var m, n = h(), k()` from `m`, and the call in the function-local const from its function
    /// `host`, each hanging off the node of its caller's path, `<file>::<path>` (spec 119
    /// criterion 8).
    #[cfg(feature = "symbols")]
    graph_build_makes_each_go_definition_a_node_of_its_kind_and_none_of_a_function_local_var:
        common::cli::assert_built_graph_holds(
            ("f.go", "go", common::fixtures::GO_DEFINITIONS_SOURCE),
            &[
                ("Alias", "Alias", "type", 9),
                ("Base", "Base", "type", 7),
                ("Drawer", "Drawer", "type", 10),
                ("Drawer::Draw", "Draw", "declaration", 10),
                ("Limit", "Limit", "constant", 2),
                ("Shape", "Shape", "type", 8),
                ("Shape.Base", "Base", "field", 8),
                ("Shape.Gen", "Gen", "field", 8),
                ("Shape.Ptr", "Ptr", "field", 8),
                ("Shape.Qual", "Qual", "field", 8),
                ("Shape.side", "side", "field", 8),
                ("Shape.x", "x", "field", 8),
                ("Shape.y", "y", "field", 8),
                ("Shape::area", "area", "method", 12),
                ("Shape::bare", "bare", "declaration", 14),
                ("a", "a", "constant", 3),
                ("b", "b", "constant", 3),
                ("ext", "ext", "declaration", 13),
                ("grouped", "grouped", "variable", 5),
                ("host", "host", "function", 11),
                ("host::inner", "inner", "constant", 11),
                ("m", "m", "variable", 6),
                ("n", "n", "variable", 6),
                ("single", "single", "variable", 4),
            ],
            &["f", "g", "h", "k", "probe"],
            &[
                ("f.go::a", "f.go::%f"),
                ("f.go::a", "f.go::%g"),
                ("f.go::host", "f.go::%probe"),
                ("f.go::m", "f.go::%h"),
                ("f.go::m", "f.go::%k"),
            ],
            "every definition of the built graph at its path, none for a function-local var or \
             the alias named _, one placeholder per undefined name a call reaches",
        );
}
