//! Periphery (contract and integration) tests for spec 116 criterion 6, RUST DEFINITIONS ARE
//! NODES: every Rust definition type is a definition of its kind. The crate's own tests pin the
//! rule on the extracted symbols; these run OUTSIDE the crate and guard the two boundaries the
//! rule crosses on its way to an operator:
//!
//!  - the graph `rigger graph build` folds from the Rust definitions fixture: each definition is
//!    a node carrying its kind string and line, a test-attributed member is no node while its
//!    owner and its siblings are, and each call hangs off the caller the fixture gives it;
//!  - the persisted symbol index: the variant names the three kinds a Rust data definition
//!    takes, `static`, `variant` and `field`, are written under and read back from.

mod common;
#[path = "common/symbol_index.rs"]
mod symbol_index;

/// GIVEN a project whose one Rust file is the Rust definitions fixture,
/// WHEN the operator runs `rigger graph build`,
/// THEN the graph's code entities are exactly one node per leaf a production definition gives,
/// each carrying exactly its kind and line, and one placeholder per name a reference reaches
/// that no definition gives (`X`, `build`, `compute`, `seed`):
///
///  - an item const and a fn-local one are a `constant`, an item static and a fn-local one a
///    `static`, each variant a `variant`, each named field a `field`, the trait a `trait`, its
///    associated type a `type`, its required fn a `declaration`, the proc-macro fn and the
///    `macro_rules!` a `macro`, and each `extern crate` a `module`, the aliased one at its alias
///    `base` with no node at `core`;
///  - the tuple fields are the nodes `0` and `1`: the two first fields of the tuple struct and
///    of the tuple variant share `0`, which settles at the lower line;
///  - a fn and a field of one leaf share its node, a `function` at the fn's line whichever is
///    above (`size`, `area`), and the field and the method `real` share one, a `method`;
///  - no test-attributed member is a node (`2`, `Probe`, `spy`, `PROBE_LIMIT`, `PROBE_TABLE`,
///    `probe`, `A`, `t`, `inner`), while the struct, enum, impl, fn and mod whose body opens
///    with one (`S`, `E`, `impl X`, `f`, `outer`) and the sibling beside it (`B`, `real`) are.
///
/// Its `CALLS` edges are exactly: the call in the item static's initializer from the static,
/// the one in the const inside the proc-macro fn from the macro, the one in the fn-local const
/// from its fn, and each impl header's self type from the impl.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_makes_each_rust_definition_a_node_of_its_kind_and_none_of_a_test_member() {
    use common::fixtures::{attrs_of, definition, live_calls, live_node_ids};
    use rigger::contextgraph::KIND_CODE_ENTITY;

    let (_project, graph) =
        common::cli::built_graph(&[("f.rs", common::fixtures::RUST_DEFINITIONS_SOURCE)]);

    let definitions = [
        ("0", "field", 5),
        ("1", "field", 5),
        ("Alias", "type", 8),
        ("B", "variant", 22),
        ("Bits", "type", 7),
        ("CACHE", "static", 14),
        ("Canvas", "type", 9),
        ("Draw", "trait", 9),
        ("E", "type", 22),
        ("LIMIT", "constant", 3),
        ("LOCAL", "constant", 14),
        ("Pair", "type", 5),
        ("Plot", "type", 17),
        ("S", "type", 21),
        ("SEED", "constant", 13),
        ("Shape", "type", 6),
        ("Struct", "variant", 6),
        ("TABLE", "static", 4),
        ("Tuple", "variant", 6),
        ("Unit", "variant", 6),
        ("WIDTH", "constant", 10),
        ("alloc", "module", 1),
        ("area", "function", 18),
        ("base", "module", 2),
        ("derive", "macro", 13),
        ("f", "function", 24),
        ("host", "function", 14),
        ("impl Pair", "impl", 10),
        ("impl X", "impl", 23),
        ("nest", "module", 15),
        ("outer", "module", 25),
        ("paint", "declaration", 9),
        ("real", "method", 23),
        ("shout", "macro", 11),
        ("side", "field", 6),
        ("size", "function", 16),
        ("whole", "field", 7),
        ("width", "method", 10),
    ];
    let placeholders = ["X", "build", "compute", "seed"];
    let mut entities: Vec<String> = definitions
        .iter()
        .map(|(name, _, _)| *name)
        .chain(placeholders)
        .map(|name| format!("f.rs::{name}"))
        .collect();
    entities.sort();
    assert_eq!(
        live_node_ids(&graph.whole().unwrap(), KIND_CODE_ENTITY),
        entities,
        "every code entity of the built graph: one per production leaf, none for a \
         test-attributed member, one placeholder per undefined name a reference reaches"
    );
    assert_eq!(
        definitions.map(|(name, _, _)| attrs_of(&graph, &format!("f.rs::{name}"))),
        definitions.map(|(name, kind, line)| definition(name, kind, line)),
        "each definition node carries its kind and line, and nothing else"
    );
    let calls: Vec<(String, String)> = [
        ("f.rs::TABLE", "f.rs::build"),
        ("f.rs::derive", "f.rs::seed"),
        ("f.rs::host", "f.rs::compute"),
        ("f.rs::impl Pair", "f.rs::Pair"),
        ("f.rs::impl X", "f.rs::X"),
    ]
    .map(|(from, to)| (from.to_string(), to.to_string()))
    .to_vec();
    assert_eq!(
        live_calls(&graph),
        calls,
        "every CALLS edge of the built graph, each from the caller the fixture gives it"
    );
}

/// The persisted index writes a static, a variant and a field under the variant names
/// `Static`, `Variant` and `Field`, and reads each back as the kind it was saved as.
#[test]
fn the_three_rust_data_kinds_persist_under_their_variant_names_and_reload() {
    use rigger::grounder::symbols::model::Kind;
    use symbol_index::{def, rust_file, SavedIndex};

    let file = || {
        rust_file(
            vec![
                def(Kind::Field, "f"),
                def(Kind::Static, "s"),
                def(Kind::Variant, "v"),
            ],
            vec![],
        )
    };
    let saved = SavedIndex::save("a.rs", file());
    assert_eq!(saved.written_kinds("a.rs"), ["Field", "Static", "Variant"]);
    assert_eq!(saved.reload().files()["a.rs"], file());
}
