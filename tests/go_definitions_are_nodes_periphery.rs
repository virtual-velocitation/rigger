//! Periphery (contract and integration) tests for spec 116 criterion 7, GO DEFINITIONS ARE
//! NODES: every Go definition type is a definition of its kind. The crate's own tests pin the
//! rule on the extracted symbols; these run OUTSIDE the crate and guard the two boundaries the
//! rule crosses on its way to an operator:
//!
//!  - the graph `rigger graph build` folds from the Go definitions fixture: each definition is a
//!    node carrying its kind string and line, a function-local `var` and an alias named `_` are
//!    no node, and each reference hangs off the caller the fixture gives it;
//!  - the persisted symbol index: the variant name `Variable`, which the kind a package-level
//!    `var` takes is written under and read back from.

mod common;
#[path = "common/symbol_index.rs"]
mod symbol_index;

rigger::test_cases! {
    /// GIVEN a project whose one Go file is the Go definitions fixture,
    /// WHEN the operator runs `rigger graph build`,
    /// THEN the graph's definitions are exactly one node per definition, at its `<file>::<path>` id
    /// (spec 119, IDS FOLLOW THE LANGUAGE'S PATH), each carrying exactly its leaf, kind and line,
    /// and its placeholders one per name a reference reaches that no call target of the file gives (`Canvas`, `Coord`, `Gen`, `Ptr`, `Qual`, `Side`, `_`, `f`,
    /// `g`, `h`, `int`, `k`, `probe`), a field being no call target:
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
    ///  - the alias `Alias` is a `type`, and the alias named `_` no definition: `_` is the
    ///    placeholder its name's type reference reaches;
    ///  - the interface's method `Drawer::Draw`, the function `ext` and the method `Shape::bare`,
    ///    none with a body, are each a `declaration`, and the method `Shape::area` a `method`, each
    ///    method under its receiver's type.
    ///
    /// Its `CALLS` edges are exactly: both calls of `const a, b = f(), g()` from `a` and both of
    /// `var m, n = h(), k()` from `m`; each embedded field's reference to its type from the field,
    /// onto the type `Base`'s node for `Shape.Base` and onto its leaf's placeholder for the fields
    /// `Gen`, `Ptr` and `Qual`, the generic one's type argument too; the named field's type from
    /// the field and that of the declaration naming two from `x`; the alias's name and its target
    /// from the alias; the call in the function-local const from its function; the interface
    /// method's parameter type from the interface; and the method's receiver type from the method.
    /// A reference no definition encloses (the parameter and receiver types of the two bodiless
    /// declarations, both names of the alias named `_`) is no edge. Each edge hangs off the node of
    /// its caller's path, `<file>::<path>` (spec 119 criterion 8), so a field's or a method's is
    /// that definition's own node (`Shape.Gen`, `Shape.Ptr`, `Shape.Qual`, `Shape.side`,
    /// `Shape.x`, `Shape::area`), and the embedded field `Shape.Base`'s is its own, apart from the
    /// type `Base`'s.
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
            &[
                "Canvas", "Coord", "Gen", "Ptr", "Qual", "Side", "_", "f", "g", "h", "int", "k",
                "probe",
            ],
            &[
                ("f.go::Alias", "f.go::Alias"),
                ("f.go::Alias", "f.go::Base"),
                ("f.go::Drawer", "f.go::%Canvas"),
                ("f.go::Shape.Base", "f.go::Base"),
                ("f.go::Shape.Gen", "f.go::%Gen"),
                ("f.go::Shape.Gen", "f.go::%int"),
                ("f.go::Shape.Ptr", "f.go::%Ptr"),
                ("f.go::Shape.Qual", "f.go::%Qual"),
                ("f.go::Shape.side", "f.go::%Side"),
                ("f.go::Shape.x", "f.go::%Coord"),
                ("f.go::Shape::area", "f.go::Shape"),
                ("f.go::a", "f.go::%f"),
                ("f.go::a", "f.go::%g"),
                ("f.go::host", "f.go::%probe"),
                ("f.go::m", "f.go::%h"),
                ("f.go::m", "f.go::%k"),
            ],
            "every definition of the built graph at its path, none for a function-local var or \
             the alias named _, one placeholder per undefined name a reference reaches",
        );
    /// The persisted index writes a variable under the variant name `Variable`, and reads it back
    /// as the kind it was saved as.
    the_variable_kind_persists_under_its_variant_name_and_reloads:
        symbol_index::assert_kinds_persist_under_their_variant_names(
            "a.go",
            &[(rigger::grounder::symbols::model::Kind::Variable, "v", "Variable")],
        );
}
