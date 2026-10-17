//! Periphery (contract and integration) tests for spec 116 criterion 6, RUST DEFINITIONS ARE
//! NODES: every Rust definition type is a definition of its kind. The crate's own tests pin the
//! rule on the extracted symbols; these run OUTSIDE the crate and guard the two boundaries the
//! rule crosses on its way to an operator:
//!
//!  - the graph `rigger graph build` folds from the Rust definitions fixture: each definition is
//!    a node carrying its kind string and line, a test-attributed member is no node while its
//!    owner and its siblings are, and each call hangs off the caller the fixture gives it;
//!  - the persisted symbol index: the variant names `Static`, `Variant` and `Field`, which the
//!    three kinds a Rust data definition takes are written under and read back from.

mod common;
#[path = "common/symbol_index.rs"]
mod symbol_index;

rigger::test_cases! {
    /// GIVEN a project whose one Rust file is the Rust definitions fixture,
    /// WHEN the operator runs `rigger graph build`,
    /// THEN the graph's code entities are exactly one node per production definition, at its
    /// `<file>::<path>` id (spec 119, IDS FOLLOW THE LANGUAGE'S PATH), each carrying exactly its
    /// leaf, kind and line, and one placeholder per name a reference reaches that no definition
    /// gives (`X`, `build`, `compute`, `seed`):
    ///
    ///  - an item const and a fn-local one are a `constant`, an item static and a fn-local one a
    ///    `static`, each variant a `variant`, each named field a `field`, the trait a `trait`, its
    ///    associated type a `type`, its required fn a `declaration`, the proc-macro fn and the
    ///    `macro_rules!` a `macro`, and each `extern crate` a `module`, the aliased one at its
    ///    alias `base` with no node at `core`;
    ///  - each tuple field is the node of its index under its owner (`Pair.0`, `Pair.1`,
    ///    `Shape::Tuple.0`), a variant's field under its variant (`Shape::Struct.side`), an
    ///    associated item under its impl's self type (`Pair::WIDTH`, `X::real`) and a fn-local item
    ///    under its fn (`host::LOCAL`, `derive::SEED`);
    ///  - a fn and a same-named field are two nodes (`size` and `Plot.size`, `area` and
    ///    `Plot.area`), as are a field and a same-named method (`S.real`, `X::real`);
    ///  - no test-attributed member is a node (`2`, `Probe`, `spy`, `PROBE_LIMIT`, `PROBE_TABLE`,
    ///    `probe`, `A`, `t`, `inner`), while the struct, enum, impl, fn and mod whose body opens
    ///    with one (`S`, `E`, `impl X`, `f`, `outer`) and the sibling beside it (`E::B`, `X::real`)
    ///    are.
    ///
    /// Its `CALLS` edges are exactly: the call in the item static's initializer from the static,
    /// the one in the const inside the proc-macro fn from the macro, the one in the fn-local const
    /// from its fn, and each impl header's self type from the impl.
    #[cfg(feature = "symbols")]
    graph_build_makes_each_rust_definition_a_node_of_its_kind_and_none_of_a_test_member:
        common::cli::assert_built_graph_holds(
            ("f.rs", "rust", common::fixtures::RUST_DEFINITIONS_SOURCE),
            &[
                ("Alias", "Alias", "type", 8),
                ("Bits", "Bits", "type", 7),
                ("Bits.whole", "whole", "field", 7),
                ("Draw", "Draw", "trait", 9),
                ("Draw::Canvas", "Canvas", "type", 9),
                ("Draw::paint", "paint", "declaration", 9),
                ("E", "E", "type", 22),
                ("E::B", "B", "variant", 22),
                ("LIMIT", "LIMIT", "constant", 3),
                ("Pair", "Pair", "type", 5),
                ("Pair.0", "0", "field", 5),
                ("Pair.1", "1", "field", 5),
                ("Pair::WIDTH", "WIDTH", "constant", 10),
                ("Pair::width", "width", "method", 10),
                ("Plot", "Plot", "type", 17),
                ("Plot.area", "area", "field", 17),
                ("Plot.size", "size", "field", 17),
                ("S", "S", "type", 21),
                ("S.real", "real", "field", 21),
                ("Shape", "Shape", "type", 6),
                ("Shape::Struct", "Struct", "variant", 6),
                ("Shape::Struct.side", "side", "field", 6),
                ("Shape::Tuple", "Tuple", "variant", 6),
                ("Shape::Tuple.0", "0", "field", 6),
                ("Shape::Unit", "Unit", "variant", 6),
                ("TABLE", "TABLE", "static", 4),
                ("X::real", "real", "method", 23),
                ("alloc", "alloc", "module", 1),
                ("area", "area", "function", 18),
                ("base", "base", "module", 2),
                ("derive", "derive", "macro", 13),
                ("derive::SEED", "SEED", "constant", 13),
                ("f", "f", "function", 24),
                ("host", "host", "function", 14),
                ("host::CACHE", "CACHE", "static", 14),
                ("host::LOCAL", "LOCAL", "constant", 14),
                ("impl Pair", "impl Pair", "impl", 10),
                ("impl X", "impl X", "impl", 23),
                ("nest", "nest", "module", 15),
                ("outer", "outer", "module", 25),
                ("shout", "shout", "macro", 11),
                ("size", "size", "function", 16),
            ],
            &["X", "build", "compute", "seed"],
            &[
                ("f.rs::TABLE", "f.rs::%build"),
                ("f.rs::derive", "f.rs::%seed"),
                ("f.rs::host", "f.rs::%compute"),
                ("f.rs::impl Pair", "f.rs::Pair"),
                ("f.rs::impl X", "f.rs::%X"),
            ],
            "every code entity of the built graph: one per production definition at its path, none \
             for a test-attributed member, one placeholder per undefined name a reference reaches",
        );
    /// The persisted index writes a static, a variant and a field under the variant names
    /// `Static`, `Variant` and `Field`, and reads each back as the kind it was saved as.
    the_three_rust_data_kinds_persist_under_their_variant_names_and_reload:
        symbol_index::assert_kinds_persist_under_their_variant_names(
            "a.rs",
            &[
                (rigger::grounder::symbols::model::Kind::Field, "f", "Field"),
                (rigger::grounder::symbols::model::Kind::Static, "s", "Static"),
                (rigger::grounder::symbols::model::Kind::Variant, "v", "Variant"),
            ],
        );
}
