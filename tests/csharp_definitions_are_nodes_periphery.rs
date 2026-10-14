//! Periphery (contract and integration) tests for spec 116 criterion 8, C# DEFINITIONS ARE
//! NODES: every C# definition type is a definition of its kind and leaf. The crate's own tests
//! pin the rule on the extracted symbols; these run OUTSIDE the crate and guard the two
//! boundaries the rule crosses on its way to an operator:
//!
//!  - the graph `rigger graph build` folds from the C# definitions fixture: each definition is a
//!    node carrying its kind string and line under an id that is the file and its leaf verbatim
//!    (a dotted namespace, `~Foo`, `+`, `this`, `implicit operator int`), a method-local variable
//!    is no node, and each reference hangs off the caller the fixture gives it;
//!  - the persisted symbol index: the variant name `Property`, which the kind a C# property
//!    takes is written under and read back from.

mod common;
#[path = "common/symbol_index.rs"]
mod symbol_index;

rigger::test_cases! {
    /// GIVEN a project whose one C# file is the C# definitions fixture,
    /// WHEN the operator runs `rigger graph build`,
    /// THEN the graph's code entities are exactly one node per leaf a definition gives, each
    /// carrying exactly its kind and line, and no placeholder, every name a reference reaches
    /// (`Probe`, `Go`) being defined:
    ///
    ///  - a class, a struct, a record, an interface, an enum and a delegate are each a `type`, and
    ///    each enum member a `variant`;
    ///  - a field is a `field`, each name of the field declaration naming two (`lo`, `hi`), the
    ///    `static readonly` one and the event field (`Changed`) included, and a `const` field a
    ///    `constant`, with (`Max`) or without (`Min`) another modifier; the method-local `local`
    ///    and `deep` are no node;
    ///  - a property (`Size`) and an event with accessors (`Moved`) are each a `property`, and the
    ///    local function `Inner` a `function`;
    ///  - the block namespace is a `module` at `A.B.C` and the file-scoped one at `S.T`, the
    ///    destructor a `method` at `~Foo`, the operator at `+`, the indexer at `this` and the
    ///    conversion operator at `implicit operator int`, its header's wide spacing collapsed;
    ///  - a member holding no body is a `declaration`: the interface's `Paint`, the abstract
    ///    `Bare`, the local function `Outer`, the destructor `~Ext`, the operator `-` and the
    ///    conversion operator `explicit operator Ext`;
    ///  - a constructor shares its class's leaf and so its node, which the kind precedence
    ///    settles a `type` at the class's line: `Foo`, its constructor four lines below, and
    ///    `Ext`, whose bodiless constructor, a `declaration`, shares the class's line.
    ///
    /// Its `CALLS` edges are exactly: the struct's two field type references from the struct and
    /// the field initializer's from the field (`made`); the event accessor's from the event and
    /// the property value's from the property; the constructor body's from the node its class
    /// shares; the destructor's, the operator's two, the conversion operator's and the indexer's
    /// each from that member's leaf; and the local function's from the local function, the method
    /// keeping its own.
    #[cfg(feature = "symbols")]
    graph_build_makes_each_csharp_definition_a_node_of_its_kind_and_none_of_a_method_local_variable:
        common::cli::assert_built_graph_holds(
            ("f.cs", "csharp", common::fixtures::CSHARP_DEFINITIONS_SOURCE),
            &[
                ("+", "method", 14),
                ("-", "declaration", 19),
                ("A.B.C", "module", 2),
                ("Bare", "declaration", 19),
                ("Blue", "variant", 4),
                ("Cell", "type", 7),
                ("Changed", "field", 9),
                ("Ext", "type", 19),
                ("Fold", "type", 3),
                ("Foo", "type", 8),
                ("Go", "method", 2),
                ("Hue", "type", 4),
                ("IDraw", "type", 6),
                ("Inner", "function", 17),
                ("Max", "constant", 7),
                ("Min", "constant", 7),
                ("Moved", "property", 10),
                ("Outer", "declaration", 17),
                ("Paint", "declaration", 6),
                ("Probe", "type", 2),
                ("Rec", "type", 5),
                ("Red", "variant", 4),
                ("Run", "method", 17),
                ("S.T", "module", 1),
                ("Seed", "field", 7),
                ("Size", "property", 11),
                ("explicit operator Ext", "declaration", 19),
                ("hi", "field", 7),
                ("implicit operator int", "method", 15),
                ("lo", "field", 7),
                ("made", "field", 7),
                ("owner", "field", 7),
                ("this", "method", 16),
                ("~Ext", "declaration", 19),
                ("~Foo", "method", 13),
            ],
            &[],
            &[
                ("f.cs::+", "f.cs::Go"),
                ("f.cs::+", "f.cs::Probe"),
                ("f.cs::Cell", "f.cs::Probe"),
                ("f.cs::Foo", "f.cs::Probe"),
                ("f.cs::Inner", "f.cs::Probe"),
                ("f.cs::Moved", "f.cs::Probe"),
                ("f.cs::Run", "f.cs::Probe"),
                ("f.cs::Size", "f.cs::Probe"),
                ("f.cs::implicit operator int", "f.cs::Probe"),
                ("f.cs::made", "f.cs::Probe"),
                ("f.cs::this", "f.cs::Probe"),
                ("f.cs::~Foo", "f.cs::Probe"),
            ],
            "every code entity of the built graph: one per definition leaf, none for a \
             method-local variable or const, and no placeholder",
        );
    /// The persisted index writes a property under the variant name `Property`, and reads it back
    /// as the kind it was saved as.
    the_property_kind_persists_under_its_variant_name_and_reloads:
        symbol_index::assert_kinds_persist_under_their_variant_names(
            "a.cs",
            &[(rigger::grounder::symbols::model::Kind::Property, "p", "Property")],
        );
}
