//! Periphery (contract and integration) tests for spec 116 criterion 8, C# DEFINITIONS ARE
//! NODES: every C# definition type is a definition of its kind and leaf. The crate's own tests
//! pin the rule on the extracted symbols; these run OUTSIDE the crate and guard the two
//! boundaries the rule crosses on its way to an operator:
//!
//!  - the graph `rigger graph build` folds from the C# definitions fixture: each definition is a
//!    node carrying its kind string and line under an id that is the file and its path, each leaf
//!    in it verbatim (a dotted namespace, `~Foo`, `+`, `this`, `implicit operator int`), a
//!    method-local variable is no node, and each reference hangs off the caller the fixture gives
//!    it;
//!  - the persisted symbol index: the variant name `Property`, which the kind a C# property
//!    takes is written under and read back from.

mod common;
#[path = "common/symbol_index.rs"]
mod symbol_index;

rigger::test_cases! {
    /// GIVEN a project whose one C# file is the C# definitions fixture,
    /// WHEN the operator runs `rigger graph build`,
    /// THEN the graph's definitions are exactly one node per definition, at its `<file>::<path>` id
    /// (spec 119, IDS FOLLOW THE LANGUAGE'S PATH), each carrying exactly its leaf, kind and line,
    /// and no placeholder, every name a reference reaches (`Probe`, `Go`) being defined; the
    /// file-scoped namespace `S.T` holds every declaration after it, so each path begins `S.T`:
    ///
    ///  - a class, a struct, a record, an interface, an enum and a delegate are each a `type`, and
    ///    each enum member a `variant`;
    ///  - a field is a `field`, each name of the field declaration naming two (`lo`, `hi`), the
    ///    `static readonly` one and the event field (`Changed`) included, and a `const` field a
    ///    `constant`, with (`Max`) or without (`Min`) another modifier; the method-local `local`
    ///    and `deep` are no node;
    ///  - a property (`Size`) and an event with accessors (`Moved`) are each a `property`, and the
    ///    local function `Inner` a `function`;
    ///  - the block namespace is a `module` at `S.T::A.B.C` and the file-scoped one at `S.T`, the
    ///    destructor a `method` at `S.T::Foo::~Foo`, the operator at `S.T::Foo::+`, the indexer at
    ///    `S.T::Foo::this` and the conversion operator at `S.T::Foo::implicit operator int`, its
    ///    header's wide spacing collapsed;
    ///  - a field, an event field and a property sit under their type after a `.`
    ///    (`S.T::Cell.lo`, `S.T::Foo.Changed`, `S.T::Foo.Size`), a `const` field after a `::`
    ///    (`S.T::Cell::Max`), and a local function under its method (`S.T::Foo::Run::Inner`);
    ///  - a member holding no body is a `declaration`: the interface's `Paint`, the abstract
    ///    `Bare`, the local function `Outer`, the destructor `~Ext`, the operator `-` and the
    ///    conversion operator `explicit operator Ext`;
    ///  - a constructor is a node under its class at the class's leaf: `S.T::Foo::Foo`, a
    ///    `constructor` four lines below its class, and `S.T::Ext::Ext`, a `declaration` on its
    ///    class's line.
    ///
    /// Its `CALLS` edges are exactly: the struct's two field type references from the struct and
    /// the field initializer's from the field (`made`); the event accessor's from the event and
    /// the property value's from the property; the constructor body's from the constructor; the
    /// destructor's, the operator's two, the conversion operator's and the indexer's each from that
    /// member; and the local function's from the local function, the method keeping its own. Each
    /// lands on the one same-file definition of its leaf (`S.T::A.B.C::Probe`,
    /// `S.T::A.B.C::Probe::Go`) and hangs off the node of its caller's path, `<file>::<path>`
    /// (spec 119 criterion 8).
    #[cfg(feature = "symbols")]
    graph_build_makes_each_csharp_definition_a_node_of_its_kind_and_none_of_a_method_local_variable:
        common::cli::assert_built_graph_holds(
            ("f.cs", "csharp", common::fixtures::CSHARP_DEFINITIONS_SOURCE),
            &[
                ("S.T", "S.T", "module", 1),
                ("S.T::A.B.C", "A.B.C", "module", 2),
                ("S.T::A.B.C::Probe", "Probe", "type", 2),
                ("S.T::A.B.C::Probe::Go", "Go", "method", 2),
                ("S.T::Cell", "Cell", "type", 7),
                ("S.T::Cell.Seed", "Seed", "field", 7),
                ("S.T::Cell.hi", "hi", "field", 7),
                ("S.T::Cell.lo", "lo", "field", 7),
                ("S.T::Cell.made", "made", "field", 7),
                ("S.T::Cell.owner", "owner", "field", 7),
                ("S.T::Cell::Max", "Max", "constant", 7),
                ("S.T::Cell::Min", "Min", "constant", 7),
                ("S.T::Ext", "Ext", "type", 19),
                ("S.T::Ext::-", "-", "declaration", 19),
                ("S.T::Ext::Bare", "Bare", "declaration", 19),
                ("S.T::Ext::Ext", "Ext", "declaration", 19),
                ("S.T::Ext::explicit operator Ext", "explicit operator Ext", "declaration", 19),
                ("S.T::Ext::~Ext", "~Ext", "declaration", 19),
                ("S.T::Fold", "Fold", "type", 3),
                ("S.T::Foo", "Foo", "type", 8),
                ("S.T::Foo.Changed", "Changed", "field", 9),
                ("S.T::Foo.Moved", "Moved", "property", 10),
                ("S.T::Foo.Size", "Size", "property", 11),
                ("S.T::Foo::+", "+", "method", 14),
                ("S.T::Foo::Foo", "Foo", "constructor", 12),
                ("S.T::Foo::Run", "Run", "method", 17),
                ("S.T::Foo::Run::Inner", "Inner", "function", 17),
                ("S.T::Foo::Run::Outer", "Outer", "declaration", 17),
                ("S.T::Foo::implicit operator int", "implicit operator int", "method", 15),
                ("S.T::Foo::this", "this", "method", 16),
                ("S.T::Foo::~Foo", "~Foo", "method", 13),
                ("S.T::Hue", "Hue", "type", 4),
                ("S.T::Hue::Blue", "Blue", "variant", 4),
                ("S.T::Hue::Red", "Red", "variant", 4),
                ("S.T::IDraw", "IDraw", "type", 6),
                ("S.T::IDraw::Paint", "Paint", "declaration", 6),
                ("S.T::Rec", "Rec", "type", 5),
            ],
            &[],
            &[
                ("f.cs::S.T::Cell", "f.cs::S.T::A.B.C::Probe"),
                ("f.cs::S.T::Cell.made", "f.cs::S.T::A.B.C::Probe"),
                ("f.cs::S.T::Foo.Moved", "f.cs::S.T::A.B.C::Probe"),
                ("f.cs::S.T::Foo.Size", "f.cs::S.T::A.B.C::Probe"),
                ("f.cs::S.T::Foo::+", "f.cs::S.T::A.B.C::Probe"),
                ("f.cs::S.T::Foo::+", "f.cs::S.T::A.B.C::Probe::Go"),
                ("f.cs::S.T::Foo::Foo", "f.cs::S.T::A.B.C::Probe"),
                ("f.cs::S.T::Foo::Run", "f.cs::S.T::A.B.C::Probe"),
                ("f.cs::S.T::Foo::Run::Inner", "f.cs::S.T::A.B.C::Probe"),
                ("f.cs::S.T::Foo::implicit operator int", "f.cs::S.T::A.B.C::Probe"),
                ("f.cs::S.T::Foo::this", "f.cs::S.T::A.B.C::Probe"),
                ("f.cs::S.T::Foo::~Foo", "f.cs::S.T::A.B.C::Probe"),
            ],
            "every definition of the built graph at its path, none for a method-local variable or \
             const, and no placeholder",
        );
    /// The persisted index writes a property under the variant name `Property`, and reads it back
    /// as the kind it was saved as.
    the_property_kind_persists_under_its_variant_name_and_reloads:
        symbol_index::assert_kinds_persist_under_their_variant_names(
            "a.cs",
            &[(rigger::grounder::symbols::model::Kind::Property, "p", "Property")],
        );
}
