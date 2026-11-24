//! Periphery (API / contract / integration) tests for spec 117, REFERENCES ARE EXTRACTED AND
//! ISOLATED: a Rust const read, type reference and `use` are extracted tagged with their
//! `ref_kind`, and no consumer that counts calls - the fold's `REFERENCES` and `CALLS` edges among
//! them - reads one. The crate's own tests pin the extraction, the emit pass, the grounder's
//! counts and the fold arm one module at a time over hand-built input; these run OUTSIDE the
//! crates and guard the boundary a tagged reference crosses on its way to an operator:
//!
//!  - the graph `rigger graph build` folds: a real tree's tagged references fold no placeholder
//!    and no edge of the event arm's, each landing instead as the staged row's one edge onto its
//!    one definition (spec 117, STAGED REFERENCES), while its call folds as before; that edge
//!    lands only while one candidate of its reference's kind answers it - the same file's before
//!    the project's, `extracted` - stands with its dates while a re-extraction keeps its
//!    definition, retires with it and lands again on its return, and
//!    its file, never its fn, is listed referenced but not called; a file
//!    rewritten to a `use` that nothing defines keeps its file node and loses every edge, and a
//!    new `use`-only file gains its file node; and the tagged
//!    references of a test file, of a `#[cfg(test)]` module or under their own `#[cfg(test)]`
//!    fold no edge and prove no definition, while their call proves one;
//!  - through CAPTURES, a type that the Go, TypeScript, TSX or C# tags query captures as a
//!    reference outside a constructor folds no placeholder and no `CALLS` edge, landing instead as
//!    one `REFERENCES` edge onto its one same-file definition, and nowhere when nothing defines it;
//!  - a constructor stays a call: a TypeScript, TSX, JavaScript or C# `new Square()` lands as
//!    the call's `REFERENCES` and `CALLS` edges onto its one same-file class, no placeholder;
//!  - a qualifier capture names the mention: a TypeScript, TSX or JavaScript `new Ns.Box()`
//!    and a C# base `Ns.IShape` or `List<IShape>` are stored as the mentions `Ns.Box` of the leaf
//!    `Box`, `Ns.IShape` of `IShape` and `List` of `List`, and land on the one definition the name
//!    as written reaches - the member qualifier's fallback to the one same-file `Box`, the
//!    `namespace Ns` member over its same-file namesake, and `List`, the leaf before the type
//!    arguments;
//!  - and Rust writes the qualifier: `Store::open()`, `Self::new()` inside `impl<T> Store<T>` and
//!    `self.file()` inside `trait CodeView` are stored as the calls `Store::open`, `Store::new` and
//!    `CodeView.file`, while `self.inner.open()` and `paths::module_dir()` are stored bare, and
//!    each lands where its name reaches - nowhere for a qualifier naming no type of the tree, and
//!    the trait's own `file` over its same-file namesake.

#[cfg(feature = "symbols")]
use crate::common;
#[cfg(feature = "symbols")]
use crate::symbol_index;

#[cfg(feature = "symbols")]
use common::fixtures::{REFERENCE_KINDS_DEFINER as DEFINER, REFERENCE_KINDS_USER as USER};
#[cfg(feature = "symbols")]
use symbol_index::USE_ONLY_SOURCE;

/// The file the use-only rebuild adds, holding [`USE_ONLY_SOURCE`].
#[cfg(feature = "symbols")]
const LEDGER: &str = "src/ledger.rs";

/// The placeholder the user file's call to `draw` lands on, `draw` being defined in another file.
#[cfg(feature = "symbols")]
const DRAW_PLACEHOLDER: &str = "src/user.rs::%draw";

/// The code-entity ids of the definitions of the reference-kinds tree's definer file.
#[cfg(feature = "symbols")]
const DEFINER_ENTITIES: [&str; 3] = [
    "src/shapes.rs::Kestrel",
    "src/shapes.rs::QUOTA_CAP",
    "src/shapes.rs::draw",
];

/// The code-entity id of the user file's `fn paint`, the fn its type reference and const read
/// are staged from.
#[cfg(feature = "symbols")]
const PAINT: &str = "src/user.rs::paint";

/// The `REFERENCES` and `CALLS` edges a built reference-kinds tree serves, sorted as
/// [`common::fixtures::landed`] answers them: the user file's call to `draw` on its placeholder,
/// and one `REFERENCES` edge per `(from, to, tier)` of `references`.
#[cfg(feature = "symbols")]
fn the_call_and(references: &[(&str, &str, &str)]) -> Vec<(String, String, String, String)> {
    let mut landed = common::fixtures::a_call_from(
        USER,
        "paint",
        DRAW_PLACEHOLDER,
        rigger::contextgraph::TIER_INFERRED,
    );
    for (from, to, tier) in references {
        landed.push(common::fixtures::reference(from, to, tier));
    }
    landed.sort();
    landed
}

/// GIVEN a project whose `src/shapes.rs` defines `Kestrel`, `QUOTA_CAP` and `draw` and whose
/// `src/user.rs` imports `Kestrel` and, in `fn paint`, takes it as a parameter type, calls `draw`
/// and reads `QUOTA_CAP`,
/// WHEN the operator runs `rigger graph build`,
/// THEN the graph's `REFERENCES` and `CALLS` edges are the call's two, landed on its placeholder
/// at the tier a cross-file definition earns, and one `inferred` `REFERENCES` edge for each staged
/// row onto its one definition - the file's `use` of `Kestrel`, and `fn paint`'s type reference
/// to `Kestrel` and const read of `QUOTA_CAP` - and its code entities are the definitions and that
/// one placeholder: the `use`, the type reference and the const read fold no node.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_folds_a_files_call_and_one_edge_per_staged_reference_and_no_placeholder() {
    use rigger::contextgraph::TIER_INFERRED;

    let (_project, graph) = common::cli::built_graph(&common::fixtures::REFERENCE_KINDS_TREE);

    assert_eq!(
        common::fixtures::landed(&graph),
        the_call_and(&[
            (USER, DEFINER_ENTITIES[0], TIER_INFERRED),
            (PAINT, DEFINER_ENTITIES[0], TIER_INFERRED),
            (PAINT, DEFINER_ENTITIES[1], TIER_INFERRED),
        ]),
        "the call to `draw` lands as before, and each staged reference on its one definition"
    );
    assert_eq!(
        common::fixtures::live_file_and_entity_ids(&graph.whole().unwrap()),
        common::fixtures::file_and_entity_ids(
            &[DEFINER, USER],
            &[&DEFINER_ENTITIES[..], &[DRAW_PLACEHOLDER, PAINT]].concat()
        ),
        "no placeholder stands for `Kestrel` or `QUOTA_CAP`"
    );
}

/// GIVEN the built project of the test above,
/// WHEN the operator rewrites `src/user.rs` to a `use` alone, adds `src/ledger.rs` holding the
/// same `use` alone and runs `rigger graph build` again,
/// THEN no `REFERENCES` or `CALLS` edge stands, `src/user.rs` keeps its file node though its
/// `fn paint` and the call's placeholder are gone, and `src/ledger.rs` gains its file node.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_over_use_only_files_folds_their_file_nodes_and_no_edge() {
    let (project, _) = common::cli::built_graph(&common::fixtures::REFERENCE_KINDS_TREE);
    let graph = common::cli::rebuilt_graph(
        project.path(),
        &[(USER, USE_ONLY_SOURCE), (LEDGER, USE_ONLY_SOURCE)],
    );

    assert_eq!(
        common::fixtures::landed(&graph),
        [],
        "a `use` nothing defines folds no edge"
    );
    assert_eq!(
        common::fixtures::live_file_and_entity_ids(&graph.whole().unwrap()),
        common::fixtures::file_and_entity_ids(&[LEDGER, DEFINER, USER], &DEFINER_ENTITIES),
        "each `use`-only file stands as its file node alone"
    );
}

/// GIVEN the built reference-kinds tree, whose `fn paint` reads `QUOTA_CAP` and names `Kestrel`
/// and whose file imports `Kestrel`, calling neither,
/// WHEN a reader walks UP from each definition,
/// THEN each lists `src/user.rs` referenced but not called, the file of the fn its row is staged
/// from, never `fn paint` itself.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_lists_a_staged_reference_s_file_never_its_fn_as_referenced_not_called() {
    let (_project, graph) = common::cli::built_graph(&common::fixtures::REFERENCE_KINDS_TREE);

    assert_eq!(
        [DEFINER_ENTITIES[0], DEFINER_ENTITIES[1]].map(|seed| {
            common::fixtures::referenced_not_called_ids(
                &common::fixtures::records_graph(&graph),
                seed,
            )
        }),
        [[USER], [USER]],
        "a fn-level row lists its file"
    );
}

/// `src/shapes.rs` of the reference-kinds tree with its `QUOTA_CAP` definition removed.
#[cfg(feature = "symbols")]
const DEFINER_WITHOUT_QUOTA_CAP: &str = "pub struct Kestrel;\npub fn draw() {}\n";

/// GIVEN the built reference-kinds tree,
/// WHEN the operator removes `QUOTA_CAP` from `src/shapes.rs` and rebuilds, then restores it and
/// rebuilds again,
/// THEN the first rebuild retires `fn paint`'s edge onto `QUOTA_CAP` alone, leaving no reader of
/// it, and the second lands that edge again, every `Kestrel` edge standing throughout.
#[cfg(feature = "symbols")]
#[test]
fn graph_rebuild_retires_a_staged_reference_with_its_definition_and_lands_it_on_its_return() {
    use rigger::contextgraph::TIER_INFERRED;

    let (project, _) = common::cli::built_graph(&common::fixtures::REFERENCE_KINDS_TREE);
    let kestrel = [
        (USER, DEFINER_ENTITIES[0], TIER_INFERRED),
        (PAINT, DEFINER_ENTITIES[0], TIER_INFERRED),
    ];

    let removed =
        common::cli::rebuilt_graph(project.path(), &[(DEFINER, DEFINER_WITHOUT_QUOTA_CAP)]);
    assert_eq!(
        common::fixtures::landed(&removed),
        the_call_and(&kestrel),
        "the const read's edge retires with its one definition"
    );
    assert_eq!(
        common::fixtures::referenced_not_called_ids(
            &common::fixtures::records_graph(&removed),
            DEFINER_ENTITIES[1]
        ),
        Vec::<String>::new(),
        "nothing references a removed definition"
    );
    drop(removed);

    let restored =
        common::cli::rebuilt_graph(project.path(), &[common::fixtures::REFERENCE_KINDS_TREE[0]]);
    assert_eq!(
        common::fixtures::landed(&restored),
        the_call_and(&[
            kestrel[0],
            kestrel[1],
            (PAINT, DEFINER_ENTITIES[1], TIER_INFERRED)
        ]),
        "the restored definition answers the const read again"
    );
}

/// The live `REFERENCES` edges of `graph` onto the reference-kinds tree's `Kestrel` and
/// `QUOTA_CAP`, each as [`common::fixtures::live_edges`] answers it, its `valid_from` and `source`
/// included.
#[cfg(feature = "symbols")]
fn staged_edges(
    graph: &(impl rigger::contextgraph::Neighbors + ?Sized),
) -> Vec<(String, String, String, i64, u64)> {
    common::fixtures::live_edges(graph)
        .into_iter()
        .filter(|(_, rel, to, ..)| {
            rel == rigger::contextgraph::REL_REFERENCES
                && DEFINER_ENTITIES[..2].contains(&to.as_str())
        })
        .collect()
}

/// GIVEN the built reference-kinds tree,
/// WHEN the operator appends `fn sketch` to `src/shapes.rs`, keeping every definition, and
/// rebuilds,
/// THEN the rebuild re-extracts the file - `sketch` stands - and each staged reference's edge onto
/// `Kestrel` and `QUOTA_CAP` stands unchanged, its `valid_from` and `source` included.
#[cfg(feature = "symbols")]
#[test]
fn graph_rebuild_of_a_definer_keeping_its_definitions_leaves_each_staged_edge_and_its_dates() {
    let (project, built) = common::cli::built_graph(&common::fixtures::REFERENCE_KINDS_TREE);
    let before = staged_edges(&built);
    drop(built);

    let [(definer, source), _] = common::fixtures::REFERENCE_KINDS_TREE;
    let edited = common::cli::rebuilt_graph(
        project.path(),
        &[(definer, &format!("{source}pub fn sketch() {{}}\n"))],
    );
    assert_eq!(
        common::fixtures::live_file_and_entity_ids(&edited.whole().unwrap()),
        common::fixtures::file_and_entity_ids(
            &[DEFINER, USER],
            &[
                &DEFINER_ENTITIES[..],
                &["src/shapes.rs::sketch", DRAW_PLACEHOLDER, PAINT]
            ]
            .concat()
        ),
        "the edited definer is re-extracted"
    );
    assert_eq!(
        (before.len(), staged_edges(&edited)),
        (3, before),
        "a kept definition leaves its staged edges and their dates untouched"
    );
}

/// A second file of the reference-kinds tree, defining a leaf `Kestrel` too.
#[cfg(feature = "symbols")]
const TWIN: &str = "src/twin.rs";

/// GIVEN the built reference-kinds tree,
/// WHEN the operator adds `src/twin.rs` defining a second `struct Kestrel` and rebuilds, then
/// rewrites it to define `fn Kestrel` instead and rebuilds again,
/// THEN two type definitions leave both `Kestrel` rows unresolved with no edge, and a fn - a
/// candidate of a `use` but never of a type reference - lands `fn paint`'s type reference on the
/// struct again while the file's `use` stays unresolved between the struct and the fn; the const
/// read's edge stands throughout.
#[cfg(feature = "symbols")]
#[test]
fn graph_rebuild_lands_a_staged_reference_only_while_one_candidate_of_its_kind_answers_it() {
    use rigger::contextgraph::TIER_INFERRED;

    let (project, _) = common::cli::built_graph(&common::fixtures::REFERENCE_KINDS_TREE);
    let quota_cap = (PAINT, DEFINER_ENTITIES[1], TIER_INFERRED);

    let two_types = common::cli::rebuilt_graph(project.path(), &[(TWIN, "pub struct Kestrel;\n")]);
    assert_eq!(
        common::fixtures::landed(&two_types),
        the_call_and(&[quota_cap]),
        "two type candidates resolve neither `Kestrel` row"
    );
    drop(two_types);

    let a_fn = common::cli::rebuilt_graph(project.path(), &[(TWIN, "pub fn Kestrel() {}\n")]);
    assert_eq!(
        common::fixtures::landed(&a_fn),
        the_call_and(&[(PAINT, DEFINER_ENTITIES[0], TIER_INFERRED), quota_cap]),
        "a fn answers the `use` beside the struct and never the type reference"
    );
}

/// GIVEN the reference-kinds tree whose `src/user.rs` also defines its own `struct Kestrel`,
/// WHEN the operator runs `rigger graph build`,
/// THEN the file's `use` and `fn paint`'s type reference land on the same-file `Kestrel` at the
/// `extracted` tier, never on `src/shapes.rs`'s, and the const read on its one definition.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_lands_a_staged_reference_on_its_same_file_definition_extracted() {
    use rigger::contextgraph::{TIER_EXTRACTED, TIER_INFERRED};

    let [definer, (_, user_source)] = common::fixtures::REFERENCE_KINDS_TREE;
    let own_kestrel = format!("{user_source}pub struct Kestrel;\n");
    let (_project, graph) = common::cli::built_graph(&[definer, (USER, &own_kestrel)]);

    let same_file = "src/user.rs::Kestrel";
    assert_eq!(
        common::fixtures::landed(&graph),
        the_call_and(&[
            (USER, same_file, TIER_EXTRACTED),
            (PAINT, same_file, TIER_EXTRACTED),
            (PAINT, DEFINER_ENTITIES[1], TIER_INFERRED),
        ]),
        "a same-file candidate is taken before a project one"
    );
}

/// The TypeScript row's source of [`TYPE_USERS`], the TSX row's too.
#[cfg(feature = "symbols")]
const TS_TYPE_USER: &str =
    "class Kestrel {}\nfunction draw() {}\nfunction paint(k: Kestrel, w: Wren) { draw(); }\n";

/// One file per language whose registered tags query captures a type in type position as a
/// `@reference.<kind>` reference (spec 117, CAPTURES), each defining a type `Kestrel` and a
/// function `draw` and, in a function `paint`, calling `draw` and naming a `Kestrel` and a `Wren`,
/// a type nothing defines, at a site its query captures: a parameter's type in Go, TypeScript and
/// TSX, a local's in C#, where `draw` and `paint` are the methods `Draw` and `Paint` of a class
/// `Shop` and the call is `this.Draw()`. Each row is `(file, source, the id paths of its definitions, sorted,
/// paint's id path, draw's id path)`.
#[cfg(feature = "symbols")]
const TYPE_USERS: [(&str, &str, &[&str], &str, &str); 4] = [
    (
        "f.go",
        "package p\n\ntype Kestrel struct{}\n\nfunc draw() {}\n\nfunc paint(k Kestrel, w Wren) { draw() }\n",
        &["Kestrel", "draw", "paint"],
        "paint",
        "draw",
    ),
    (
        "f.ts",
        TS_TYPE_USER,
        &["Kestrel", "draw", "paint"],
        "paint",
        "draw",
    ),
    (
        "f.tsx",
        TS_TYPE_USER,
        &["Kestrel", "draw", "paint"],
        "paint",
        "draw",
    ),
    (
        "f.cs",
        "class Kestrel { }\nclass Shop\n{\n    void Draw() { }\n    void Paint() { Kestrel k = null; Wren w = null; this.Draw(); }\n}\n",
        &["Kestrel", "Shop", "Shop::Draw", "Shop::Paint"],
        "Shop::Paint",
        "Shop::Draw",
    ),
];

/// GIVEN a project whose one file, in Go, TypeScript, TSX or C# ([`TYPE_USERS`]), defines `Kestrel`
/// and `draw` and, in `paint`, calls `draw` and names a `Kestrel` and an undefined `Wren` where its
/// query captures a type (in C#, `Shop`'s methods `Draw` and `Paint`),
/// WHEN the operator runs `rigger graph build`,
/// THEN the graph's `REFERENCES` and `CALLS` edges are the call's two onto `draw` and one
/// `REFERENCES` edge from `paint` onto `Kestrel`, each `extracted`: the type reference its query's
/// capture tags lands as a staged reference on its one same-file definition, never as a call, and
/// its code entities are the definitions alone, no placeholder standing for `Kestrel` or `Wren`.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_lands_another_language_s_type_reference_as_a_staged_edge_never_a_call() {
    use rigger::contextgraph::query::entity_id;
    use rigger::contextgraph::TIER_EXTRACTED;

    for (file, source, definitions, paint, draw) in TYPE_USERS {
        let (_project, graph) = common::cli::built_graph(&[(file, source)]);

        let mut edges =
            common::fixtures::a_call_from(file, paint, &entity_id(file, draw), TIER_EXTRACTED);
        edges.push(common::fixtures::reference(
            &entity_id(file, paint),
            &entity_id(file, "Kestrel"),
            TIER_EXTRACTED,
        ));
        edges.sort();
        assert_eq!(
            common::fixtures::landed(&graph),
            edges,
            "{file}: the call lands as a call, and the type reference on its one same-file \
             definition as no call"
        );
        let ids: Vec<String> = definitions
            .iter()
            .map(|path| entity_id(file, path))
            .collect();
        assert_eq!(
            common::fixtures::live_file_and_entity_ids(&graph.whole().unwrap()),
            [vec![file.to_string()], ids],
            "{file}: no placeholder stands for `Kestrel` or the undefined `Wren`"
        );
    }
}

/// The TypeScript row's source of [`CONSTRUCTING_FILES`], the TSX and JavaScript rows' too.
#[cfg(feature = "symbols")]
const JS_CONSTRUCTING_SOURCE: &str = "class Square {}\nfunction make() { return new Square(); }\n";

/// One file per language whose registry entry names a node kind that constructs an object (spec
/// 117, CAPTURES): TypeScript, TSX, JavaScript and C#. Each defines a class `Square` with no
/// constructor of its own and constructs it, `new Square()`, in `make` (in C#, `Shop`'s method
/// `Make`, into a local declared `object`, so `Square` is named in no type position). Each row is
/// `(file, source, the id paths of its definitions, sorted, the constructing definition's id
/// path)`.
#[cfg(feature = "symbols")]
const CONSTRUCTING_FILES: [(&str, &str, &[&str], &str); 4] = [
    ("f.ts", JS_CONSTRUCTING_SOURCE, &["Square", "make"], "make"),
    ("f.tsx", JS_CONSTRUCTING_SOURCE, &["Square", "make"], "make"),
    ("f.js", JS_CONSTRUCTING_SOURCE, &["Square", "make"], "make"),
    (
        "f.cs",
        "class Square { }\nclass Shop\n{\n    void Make() { object s = new Square(); }\n}\n",
        &["Shop", "Shop::Make", "Square"],
        "Shop::Make",
    ),
];

/// GIVEN a project whose one file, in TypeScript, TSX, JavaScript or C#
/// ([`CONSTRUCTING_FILES`]), defines a class `Square` and constructs it, `new Square()`, in `make`
/// (in C#, `Shop`'s method `Make`),
/// WHEN the operator runs `rigger graph build`,
/// THEN A CONSTRUCTOR STAYS A CALL (spec 117, CAPTURES): the graph's `REFERENCES` and `CALLS`
/// edges are exactly the call's two onto `Square`, each `extracted`, the query's
/// `@reference.class` capture on the language's constructor node kind landing as a call of the
/// class, never as a type reference; and its code entities are the definitions alone, no
/// placeholder standing for `Square`.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_lands_a_constructor_as_a_call_onto_its_class() {
    use rigger::contextgraph::query::entity_id;
    use rigger::contextgraph::TIER_EXTRACTED;

    let (found, expected): (Vec<_>, Vec<_>) = CONSTRUCTING_FILES
        .into_iter()
        .map(|(file, source, definitions, maker)| {
            let (_project, graph) = common::cli::built_graph(&[(file, source)]);
            let found = (
                file,
                common::fixtures::landed(&graph),
                common::fixtures::live_file_and_entity_ids(&graph.whole().unwrap()),
            );
            let ids: Vec<String> = definitions
                .iter()
                .map(|path| entity_id(file, path))
                .collect();
            let expected = (
                file,
                common::fixtures::a_call_from(
                    file,
                    maker,
                    &entity_id(file, "Square"),
                    TIER_EXTRACTED,
                ),
                [vec![file.to_string()], ids],
            );
            (found, expected)
        })
        .unzip();
    assert_eq!(
        found, expected,
        "each `new Square()` lands as the call's two edges onto its one same-file class, no \
         placeholder standing for it"
    );
}

/// The source of [`QUALIFIED_FILES`]' TypeScript, TSX and JavaScript rows whose qualifier names
/// nothing: one class `Box`, constructed in `make` as `new Ns.Box()`.
#[cfg(feature = "symbols")]
const JS_QUALIFIED_SOURCE: &str = "class Box {}\nfunction make() { return new Ns.Box(); }\n";

/// One file per corner of a qualified reference's name (spec 117, CAPTURES), each `(file, source,
/// the id path of the definition the reference is staged from, the name of its one stored
/// mention, that mention's leaf, the id path of the one definition it lands on, whether it lands
/// as a call)`: in TypeScript, TSX and JavaScript a `new Ns.Box()` whose member qualifier `Ns`
/// names no definition, beside one `Box`; in TypeScript the same call beside a top-level `Box`
/// and the `Box` of a `namespace Ns`; in C# a base `Ns.IShape` beside the `IShape` of
/// `namespace Ns` and of `namespace Other`; and in C# a base `List<IShape>`, a name holding type
/// arguments, beside a class `List` and an interface `IShape`.
#[cfg(feature = "symbols")]
const QUALIFIED_FILES: [(&str, &str, &str, &str, &str, &str, bool); 6] = [
    ("f.ts", JS_QUALIFIED_SOURCE, "make", "Ns.Box", "Box", "Box", true),
    ("f.tsx", JS_QUALIFIED_SOURCE, "make", "Ns.Box", "Box", "Box", true),
    ("f.js", JS_QUALIFIED_SOURCE, "make", "Ns.Box", "Box", "Box", true),
    (
        "f.ts",
        "namespace Ns { export class Box {} }\nclass Box {}\nfunction make() { return new Ns.Box(); }\n",
        "make",
        "Ns.Box",
        "Box",
        "Ns::Box",
        true,
    ),
    (
        "f.cs",
        "namespace Ns { interface IShape { } }\nnamespace Other { interface IShape { } }\nclass A : Ns.IShape { }\n",
        "A",
        "Ns.IShape",
        "IShape",
        "Ns::IShape",
        false,
    ),
    (
        "f.cs",
        "class List<T> { }\ninterface IShape { }\nclass A : List<IShape> { }\n",
        "A",
        "List",
        "List",
        "List",
        false,
    ),
];

/// GIVEN a project whose one file names a definition through a qualifier or with type arguments
/// ([`QUALIFIED_FILES`]),
/// WHEN the operator runs `rigger graph build`,
/// THEN A QUALIFIER CAPTURE NAMES THE MENTION (spec 117, CAPTURES): the store holds the file's
/// one mention, named as written (`Ns.Box`, `Ns.IShape`, `List`) of its leaf (`Box`, `IShape`,
/// `List`), and the graph's `REFERENCES` and `CALLS` edges are exactly the reference's, each
/// `extracted`, onto the one definition its name as written reaches - a call's two, a type
/// reference's one from the class naming its base. The member qualifier `Ns` of `new Ns.Box()`
/// keeps no candidate and leaves the call to its one same-file `Box`, where a path qualifier
/// naming nothing would make no link; the qualifier keeps the `Box` and the `IShape` that
/// `namespace Ns` holds over their same-file namesakes, which leave a reference by its leaf alone
/// no one target; and `List<IShape>` is named and lands on `List`, its leaf before the type
/// arguments.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_lands_a_qualified_reference_where_its_name_as_written_reaches() {
    use rigger::contextgraph::query::entity_id;
    use rigger::contextgraph::TIER_EXTRACTED;

    let (found, expected): (Vec<_>, Vec<_>) = QUALIFIED_FILES
        .into_iter()
        .map(|(file, source, from, name, leaf, to, call)| {
            let (_project, graph) = common::cli::built_graph(&[(file, source)]);
            let mentions = stored_mentions(&graph, file);
            let to = entity_id(file, to);
            let edges = if call {
                common::fixtures::a_call_from(file, from, &to, TIER_EXTRACTED)
            } else {
                vec![common::fixtures::reference(
                    &entity_id(file, from),
                    &to,
                    TIER_EXTRACTED,
                )]
            };
            (
                (file, source, mentions, common::fixtures::landed(&graph)),
                (
                    file,
                    source,
                    Some(vec![(name.to_string(), leaf.to_string())]),
                    edges,
                ),
            )
        })
        .unzip();
    assert_eq!(
        found, expected,
        "each qualified or type-argument reference is stored named as written, of its leaf, and \
         lands on the one definition that name reaches"
    );
}

/// The mentions the store holds for `file` in `graph`, each as `(name as written, leaf)`, sorted;
/// none when it holds no facts for `file`.
#[cfg(feature = "symbols")]
fn stored_mentions(graph: &common::cli::BuiltGraph, file: &str) -> Option<Vec<(String, String)>> {
    use rigger_domain::code_view::CodeView;

    graph.view.file(file).map(|facts| {
        let mut mentions: Vec<_> = (facts.into_owned().mentions.into_iter())
            .map(|mention| (mention.name, mention.leaf))
            .collect();
        mentions.sort();
        mentions
    })
}

/// One Rust file `f.rs` per call shape RUST WRITES THE QUALIFIER names (spec 117, RUST
/// REFERENCES), each `(what it shows, source, its stored mentions as (name as written, leaf),
/// the calls it lands as (caller, callee), each by id path and `extracted`)`. An impl header
/// stages a call to its self type, so `impl Store` calling `Store` is a row's landing too.
#[cfg(feature = "symbols")]
type RustQualifiedCall = (
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
    &'static [(&'static str, &'static str)],
);

/// The rows of [`graph_build_names_a_rust_call_by_the_qualifier_rust_writes`].
#[cfg(feature = "symbols")]
const RUST_QUALIFIED_CALLS: [RustQualifiedCall; 5] = [
    (
        "`Store::open()`, its uppercase segment naming no type of the tree, makes no link",
        "fn open() {}\nfn main() { Store::open(); }\n",
        &[("Store", "Store"), ("Store::open", "open")],
        &[],
    ),
    (
        "`Self::new()` inside `impl<T> Store<T>` is qualified by the self type, generics stripped",
        "struct Store<T>(T);\nimpl<T> Store<T> { fn new() {} fn make() { Self::new() } }\n",
        &[("Store", "Store"), ("Store::new", "new")],
        &[("Store<T>::make", "Store<T>::new"), ("impl Store<T>", "Store")],
    ),
    (
        "`self.file()` inside `trait CodeView` is qualified by the trait, over the free `file`",
        "trait CodeView { fn file(&self) {} fn lines(&self) { self.file() } }\nfn file() {}\n",
        &[("CodeView.file", "file")],
        &[("CodeView::lines", "CodeView::file")],
    ),
    (
        "`self.inner.open()`, its receiver no bare `self`, lands on its one same-file `open`",
        "struct Inner;\nimpl Inner { fn open(&self) {} }\nstruct Store;\nimpl Store { fn reopen(&self) { self.inner.open() } }\n",
        &[("Inner", "Inner"), ("Store", "Store"), ("open", "open")],
        &[
            ("Store::reopen", "Inner::open"),
            ("impl Inner", "Inner"),
            ("impl Store", "Store"),
        ],
    ),
    (
        "`paths::module_dir()`, its segment lowercase, lands on its one same-file `module_dir`",
        "fn module_dir() {}\nfn main() { paths::module_dir(); }\n",
        &[("module_dir", "module_dir")],
        &[("main", "module_dir")],
    ),
];

/// GIVEN a project whose one Rust file `f.rs` makes a call in a shape RUST WRITES THE QUALIFIER
/// names ([`RUST_QUALIFIED_CALLS`]),
/// WHEN the operator runs `rigger graph build`,
/// THEN the store holds the call's mention named as the Rust qualifier rule writes it -
/// `Store::open` and `Store::new` as paths, `CodeView.file` as a member access, `open` and
/// `module_dir` bare - beside the mention an uppercase path segment or an impl header's self type
/// stages, and the graph's `REFERENCES` and `CALLS` edges are exactly where those names reach:
/// nowhere for `Store::open`, its qualifier naming no type of the tree, where a bare `open` lands
/// on the same-file `fn open`; the trait's own `file` for `CodeView.file`, where a bare `file`
/// lands on a placeholder beside the same-file free `file`; and the one same-file candidate for
/// each other name, which `paths::module_dir`, its qualifier naming no type, would not reach.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_names_a_rust_call_by_the_qualifier_rust_writes() {
    use rigger::contextgraph::query::entity_id;
    use rigger::contextgraph::TIER_EXTRACTED;

    let owned = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
        (pairs.iter())
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    };
    let (found, expected): (Vec<_>, Vec<_>) = RUST_QUALIFIED_CALLS
        .into_iter()
        .map(|(shows, source, mentions, calls)| {
            let (_project, graph) = common::cli::built_graph(&[("f.rs", source)]);
            let mut edges: Vec<_> = (calls.iter())
                .flat_map(|(caller, callee)| {
                    let to = entity_id("f.rs", callee);
                    common::fixtures::a_call_from("f.rs", caller, &to, TIER_EXTRACTED)
                })
                .collect();
            edges.sort();
            (
                (
                    shows,
                    stored_mentions(&graph, "f.rs"),
                    common::fixtures::landed(&graph),
                ),
                (shows, Some(owned(mentions)), edges),
            )
        })
        .unzip();
    assert_eq!(
        found, expected,
        "each Rust call is stored named by the qualifier Rust writes, and lands where that name \
         reaches"
    );
}

/// A production file whose `#[cfg(test)]` module imports `Kestrel` and, in `fn check`, takes it
/// as a parameter type, calls `draw` and reads `QUOTA_CAP`; its one production fn calls nothing.
#[cfg(feature = "symbols")]
const TEST_REGION_SOURCE: &str = "fn product() {}\n#[cfg(test)]\nmod tests {\n    use crate::shapes::Kestrel;\n    fn check(k: Kestrel) -> u32 { draw(); QUOTA_CAP }\n}\n";

/// A production file whose test code sits under its own `#[cfg(test)]` attributes: a top-level
/// `use` of `Kestrel`, and in the production `fn product` a `let` taking `Kestrel` as its type and
/// reading `QUOTA_CAP` and an expression statement calling `draw`.
#[cfg(feature = "symbols")]
const TEST_ATTRIBUTED_SOURCE: &str = "#[cfg(test)]\nuse crate::shapes::Kestrel;\npub fn product() -> u32 {\n    #[cfg(test)]\n    let k: Kestrel = QUOTA_CAP;\n    #[cfg(test)]\n    draw();\n    0\n}\n";

/// GIVEN the reference-kinds tree's definer beside test code that imports `Kestrel`, takes it as
/// a parameter type, calls `draw` and reads `QUOTA_CAP` - a test file `tests/kestrel.rs` holding
/// the user file's body, the `#[cfg(test)]` module of a production file `src/probe.rs`, or the
/// test-attributed `use`, `let` and expression statement of a production file `src/gated.rs`,
/// WHEN the operator runs `rigger graph build`,
/// THEN no `REFERENCES` or `CALLS` edge stands and `draw` alone carries the test code's evidence
/// (`proven_by`): its `use`, type reference and const read prove neither `Kestrel` nor
/// `QUOTA_CAP`.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_proves_a_definition_by_test_code_s_call_alone() {
    let [definer, (_, user_source)] = common::fixtures::REFERENCE_KINDS_TREE;
    for test_code in [
        ("tests/kestrel.rs", user_source),
        ("src/probe.rs", TEST_REGION_SOURCE),
        ("src/gated.rs", TEST_ATTRIBUTED_SOURCE),
    ] {
        let (_project, graph) = common::cli::built_graph(&[definer, test_code]);

        assert_eq!(
            common::fixtures::landed(&graph),
            [],
            "{}: test code folds no structural edge",
            test_code.0
        );
        assert_eq!(
            DEFINER_ENTITIES.map(|id| common::fixtures::proven_by_attr(&graph, id)),
            [None, None, Some("1".to_string())],
            "{}: the test code's call to `draw` is its one proof",
            test_code.0
        );
    }
}
