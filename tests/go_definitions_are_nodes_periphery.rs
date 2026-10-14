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

/// GIVEN a project whose one Go file is the Go definitions fixture,
/// WHEN the operator runs `rigger graph build`,
/// THEN the graph's code entities are exactly one node per leaf a definition gives, each
/// carrying exactly its kind and line, and one placeholder per name a reference reaches that no
/// definition gives (`Canvas`, `Coord`, `Side`, `_`, `f`, `g`, `h`, `int`, `k`, `probe`):
///
///  - each name of a const spec is a `constant`, the two of `const a, b` and the function-local
///    `inner` included; a package-level `var` is a `variable`, single (`single`), grouped
///    (`grouped`) or naming two (`m`, `n`), while the function-local `local` and grouped `deep`
///    are no node;
///  - a named field is a `field`, each name of the field declaration naming two included (`x`,
///    `y`), and an embedded field a `field` at its type's base identifier, the pointer of `Ptr`,
///    the package of `Qual` and the type argument of `Gen` stripped;
///  - the embedded field `Base` shares its node with the struct type above it, a `type` at the
///    type's line;
///  - the alias `Alias` is a `type`, and the alias named `_` no definition: `_` is the
///    placeholder its name's type reference reaches;
///  - the interface's method `Draw`, the function `ext` and the method `bare`, none with a body,
///    are each a `declaration`.
///
/// Its `CALLS` edges are exactly: both calls of `const a, b = f(), g()` from `a` and both of
/// `var m, n = h(), k()` from `m`; each embedded field's reference to its type from the field,
/// which is the node itself, the generic one's type argument too; the named field's type from
/// the field and that of the declaration naming two from `x`; the alias's name and its target
/// from the alias; the call in the function-local const from its function; the interface
/// method's parameter type from the interface; and the method's receiver type from the method.
/// A reference no definition encloses (the parameter and receiver types of the two bodiless
/// declarations, both names of the alias named `_`) is no edge.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_makes_each_go_definition_a_node_of_its_kind_and_none_of_a_function_local_var() {
    use common::fixtures::{attrs_of, definition_in, live_calls, live_node_ids};
    use rigger::contextgraph::KIND_CODE_ENTITY;

    let (_project, graph) =
        common::cli::built_graph(&[("f.go", common::fixtures::GO_DEFINITIONS_SOURCE)]);

    let definitions = [
        ("Alias", "type", 9),
        ("Base", "type", 7),
        ("Draw", "declaration", 10),
        ("Drawer", "type", 10),
        ("Gen", "field", 8),
        ("Limit", "constant", 2),
        ("Ptr", "field", 8),
        ("Qual", "field", 8),
        ("Shape", "type", 8),
        ("a", "constant", 3),
        ("area", "method", 12),
        ("b", "constant", 3),
        ("bare", "declaration", 14),
        ("ext", "declaration", 13),
        ("grouped", "variable", 5),
        ("host", "function", 11),
        ("inner", "constant", 11),
        ("m", "variable", 6),
        ("n", "variable", 6),
        ("side", "field", 8),
        ("single", "variable", 4),
        ("x", "field", 8),
        ("y", "field", 8),
    ];
    let placeholders = [
        "Canvas", "Coord", "Side", "_", "f", "g", "h", "int", "k", "probe",
    ];
    let mut entities: Vec<String> = definitions
        .iter()
        .map(|(name, _, _)| *name)
        .chain(placeholders)
        .map(|name| format!("f.go::{name}"))
        .collect();
    entities.sort();
    assert_eq!(
        live_node_ids(&graph.whole().unwrap(), KIND_CODE_ENTITY),
        entities,
        "every code entity of the built graph: one per definition leaf, none for a \
         function-local var or the alias named _, one placeholder per undefined name a \
         reference reaches"
    );
    assert_eq!(
        definitions.map(|(name, _, _)| attrs_of(&graph, &format!("f.go::{name}"))),
        definitions.map(|(name, kind, line)| definition_in("go", name, kind, line)),
        "each definition node carries its kind and line, and nothing else"
    );
    let calls: Vec<(String, String)> = [
        ("f.go::Alias", "f.go::Alias"),
        ("f.go::Alias", "f.go::Base"),
        ("f.go::Base", "f.go::Base"),
        ("f.go::Drawer", "f.go::Canvas"),
        ("f.go::Gen", "f.go::Gen"),
        ("f.go::Gen", "f.go::int"),
        ("f.go::Ptr", "f.go::Ptr"),
        ("f.go::Qual", "f.go::Qual"),
        ("f.go::a", "f.go::f"),
        ("f.go::a", "f.go::g"),
        ("f.go::area", "f.go::Shape"),
        ("f.go::host", "f.go::probe"),
        ("f.go::m", "f.go::h"),
        ("f.go::m", "f.go::k"),
        ("f.go::side", "f.go::Side"),
        ("f.go::x", "f.go::Coord"),
    ]
    .map(|(from, to)| (from.to_string(), to.to_string()))
    .to_vec();
    assert_eq!(
        live_calls(&graph),
        calls,
        "every CALLS edge of the built graph, each from the caller the fixture gives it"
    );
}

/// The persisted index writes a variable under the variant name `Variable`, and reads it back
/// as the kind it was saved as.
#[test]
fn the_variable_kind_persists_under_its_variant_name_and_reloads() {
    use rigger::grounder::symbols::model::Kind;
    use symbol_index::{def, rust_file, SavedIndex};

    let file = || rust_file(vec![def(Kind::Variable, "v")], vec![]);
    let saved = SavedIndex::save("a.go", file());
    assert_eq!(saved.written_kinds("a.go"), ["Variable"]);
    assert_eq!(saved.reload().files()["a.go"], file());
}
