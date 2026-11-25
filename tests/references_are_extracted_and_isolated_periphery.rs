//! Periphery (API / contract / integration) tests for spec 117, RUST REFERENCES: a Rust const
//! read, type reference, field use, variant use, impl header and `use` are extracted tagged with
//! their `ref_kind`, and no consumer that counts calls - the fold's `REFERENCES` and `CALLS`
//! edges among them - reads one. The crate's own tests pin the extraction, the emit pass, the
//! grounder's counts and the fold arm one module at a time over hand-built input; these run
//! OUTSIDE the crates and guard the boundary a tagged reference crosses on its way to an operator:
//!
//!  - the graph `rigger graph build` folds: a real tree's tagged references fold no placeholder
//!    and no edge of the event arm's, each landing instead as the staged row's one edge onto its
//!    one definition (spec 117, RESOLVER AT READ), while its call folds as before; that edge
//!    lands only while one candidate of its reference's kind answers it - the same file's before
//!    the project's, `extracted` - stands with its dates while a re-extraction keeps its
//!    definition, retires with it and lands again on its return, and
//!    its file, never its fn, is listed referenced but not called; a file
//!    rewritten to a `use` that nothing defines keeps its file node and loses every edge, and a
//!    new `use`-only file gains its file node; and the tagged
//!    references of a test file, of a `#[cfg(test)]` module or under their own `#[cfg(test)]`
//!    fold no edge and prove no definition, while their call proves one;
//!  - through CAPTURES, a type that the Go, TypeScript, TSX, C# or Python tags query captures as a
//!    reference outside a constructor folds no placeholder and no `CALLS` edge, landing instead as
//!    one `REFERENCES` edge onto its one same-file definition, and nowhere when nothing defines it;
//!  - a constructor stays a call: a TypeScript, TSX, JavaScript or C# `new Square()` lands as
//!    the call's `REFERENCES` and `CALLS` edges onto its one same-file class, no placeholder;
//!  - a qualifier capture names the mention: a TypeScript, TSX or JavaScript `new Ns.Box()`,
//!    a C# base `Ns.IShape` or `List<IShape>` and a Python call `Shape.area()` are stored as the
//!    mentions `Ns.Box` of the leaf `Box`, `Ns.IShape` of `IShape`, `List` of `List` and
//!    `Shape.area` of `area`, and land on the one definition the name as written reaches - the
//!    member qualifier's fallback to the one same-file `Box`, the `namespace Ns` member and
//!    `Shape`'s method over their same-file namesakes, and `List`, the leaf before the type
//!    arguments;
//!  - Rust writes the qualifier: `Store::open()`, `a::Memo::<u8>::new()`, `Self::new()` inside
//!    `impl<T> Store<T>`, `self.file()` inside `trait CodeView` and `C::f()` under
//!    `use a::B as C;` are stored as the calls `Store::open`, `Memo::new`, `Store::new`,
//!    `CodeView.file` and `B::f`, while `self.inner.open()` and `paths::module_dir()` are stored
//!    bare and `Cursor::new()` under `use std::io::Cursor;`, its qualifier excluded, is not stored
//!    at all, and each stored call lands where its name reaches - nowhere for a qualifier naming no
//!    type of the tree, and the trait's own `file` over its same-file namesake;
//!  - Rust stages its field uses: `opts.system_prompt = x`, `SpawnOpts { system_prompt }` and
//!    `self.root` inside `impl Store` are stored as the `field` mentions `system_prompt`,
//!    `SpawnOpts::system_prompt` and `Store.root`, and `opts.run()` as a call alone; the
//!    unqualified one lands on its one field `inferred`, the qualified two `extracted`, and the
//!    call as the call's two edges onto its method;
//!  - Rust stages its variant uses: `InstallOutcome::AlreadyCurrent` and `Located::Many(c) =>`
//!    are stored as the `variant` mentions `InstallOutcome::AlreadyCurrent` and `Located::Many`
//!    beside their enum's `type` mention, `Kind::RejectRecurrence { n } =>` as the `variant`
//!    mention `Kind::RejectRecurrence` and the `field` mention `RejectRecurrence::n` with no `type`
//!    mention, `Hop::Frontier(cs)` as a call and `-> Self::Target` as a `type` mention alone; each
//!    lands `extracted` on the one variant, field, enum or associated type its name narrows to,
//!    and the call as the call's two edges onto its variant;
//!  - and Rust tags its impl headers: `impl<T> CodeView for FactsView<T>` is stored as the `impl`
//!    mentions `CodeView` and `FactsView`, and `impl FactsView` as the `impl` mention
//!    `FactsView`, each enclosed by its impl, a function-local impl's by the impl and never its
//!    function, with no call and no `type` mention; each lands `extracted` from its impl on the
//!    trait or type it names, and through them `FactsView::new()` lands on the `new` that
//!    `impl FactsView` defines and `FactsView::file()` on the trait's `file`, over their same-file
//!    namesakes; a self type behind a reference or a pointer is staged by its base name, and none
//!    when that name's path is rooted at the standard library or a standard-library import, so
//!    `impl Draw for &std::path::Path` lands nothing on a project's own `Path` while
//!    `impl Draw for &crate::Path` lands on it.

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
/// TSX, a parameter's annotation in Python, a local's in C#, where `draw` and `paint` are the
/// methods `Draw` and `Paint` of a class `Shop` and the call is `this.Draw()`. Each row is `(file,
/// source, the id paths of its definitions, sorted, paint's id path, draw's id path)`.
#[cfg(feature = "symbols")]
const TYPE_USERS: [(&str, &str, &[&str], &str, &str); 5] = [
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
    (
        "f.py",
        "class Kestrel:\n    pass\n\n\ndef draw():\n    pass\n\n\ndef paint(k: Kestrel, w: Wren):\n    draw()\n",
        &["Kestrel", "draw", "paint"],
        "paint",
        "draw",
    ),
];

/// GIVEN a project whose one file, in Go, TypeScript, TSX, C# or Python ([`TYPE_USERS`]), defines
/// `Kestrel` and `draw` and, in `paint`, calls `draw` and names a `Kestrel` and an undefined `Wren`
/// where its query captures a type (in C#, `Shop`'s methods `Draw` and `Paint`),
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

/// One stored mention of a [`QUALIFIED_FILES`] row: its name, its leaf and the id path of the one
/// definition it lands on.
#[cfg(feature = "symbols")]
type Written = (&'static str, &'static str, &'static str);

/// One file per corner of a qualified reference's name (spec 117, CAPTURES), each `(file, source,
/// the id path of the definition the references are staged from, each stored mention's name, leaf
/// and the id path of the one definition it lands on, sorted by name, whether they land as
/// calls)`: in TypeScript, TSX and JavaScript a `new Ns.Box()` whose member qualifier `Ns` names
/// no definition, beside one `Box`; in TypeScript the same call beside a top-level `Box` and the
/// `Box` of a `namespace Ns`; in C# a base `Ns.IShape` beside the `IShape` of `namespace Ns` and
/// of `namespace Other`; in C# a base `List<IShape>`, a name holding type arguments, beside a
/// class `List` and an interface `IShape`, its type argument `IShape` a mention of its own; and in
/// Python a call `Shape.area()` whose member qualifier `Shape` names a class, beside the method
/// `area` of `Shape` and of `Other`.
#[cfg(feature = "symbols")]
const QUALIFIED_FILES: [(&str, &str, &str, &[Written], bool); 7] = [
    ("f.ts", JS_QUALIFIED_SOURCE, "make", &[("Ns.Box", "Box", "Box")], true),
    ("f.tsx", JS_QUALIFIED_SOURCE, "make", &[("Ns.Box", "Box", "Box")], true),
    ("f.js", JS_QUALIFIED_SOURCE, "make", &[("Ns.Box", "Box", "Box")], true),
    (
        "f.ts",
        "namespace Ns { export class Box {} }\nclass Box {}\nfunction make() { return new Ns.Box(); }\n",
        "make",
        &[("Ns.Box", "Box", "Ns::Box")],
        true,
    ),
    (
        "f.cs",
        "namespace Ns { interface IShape { } }\nnamespace Other { interface IShape { } }\nclass A : Ns.IShape { }\n",
        "A",
        &[("Ns.IShape", "IShape", "Ns::IShape")],
        false,
    ),
    (
        "f.cs",
        "class List<T> { }\ninterface IShape { }\nclass A : List<IShape> { }\n",
        "A",
        &[("IShape", "IShape", "IShape"), ("List", "List", "List")],
        false,
    ),
    (
        "f.py",
        "class Shape:\n    def area(self):\n        pass\n\n\nclass Other:\n    def area(self):\n        pass\n\n\ndef total():\n    Shape.area()\n",
        "total",
        &[("Shape.area", "area", "Shape::area")],
        true,
    ),
];

/// GIVEN a project whose one file names a definition through a qualifier or with type arguments
/// ([`QUALIFIED_FILES`]),
/// WHEN the operator runs `rigger graph build`,
/// THEN A QUALIFIER CAPTURE NAMES THE MENTION (spec 117, CAPTURES): the store holds the file's
/// mentions, each named as written (`Ns.Box`, `Ns.IShape`, `List`, `Shape.area`) of its leaf
/// (`Box`, `IShape`, `List`, `area`), and the graph's `REFERENCES` and `CALLS` edges are exactly
/// the references', each `extracted`, onto the one definition each name as written reaches - a
/// call's two, a type reference's one from the class naming its base. The member qualifier `Ns`
/// of `new Ns.Box()` keeps no candidate and leaves the call to its one same-file `Box`, where a
/// path qualifier naming nothing would make no link; the qualifier keeps the `Box` and the
/// `IShape` that `namespace Ns` holds, and the `area` that `Shape` holds, over their same-file
/// namesakes, which leave a reference by its leaf alone no one target; and `List<IShape>` is
/// named and lands on `List`, its leaf before the type arguments, while its type argument
/// `IShape` is a mention of its own, landing on `IShape`.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_lands_a_qualified_reference_where_its_name_as_written_reaches() {
    use rigger::contextgraph::query::entity_id;
    use rigger::contextgraph::TIER_EXTRACTED;

    let (found, expected): (Vec<_>, Vec<_>) = QUALIFIED_FILES
        .into_iter()
        .map(|(file, source, from, written, call)| {
            let (_project, graph) = common::cli::built_graph(&[(file, source)]);
            let mentions = stored_mentions(&graph, file);
            let edges = written
                .iter()
                .flat_map(|(_, _, to)| {
                    let to = entity_id(file, to);
                    if call {
                        common::fixtures::a_call_from(file, from, &to, TIER_EXTRACTED)
                    } else {
                        vec![common::fixtures::reference(
                            &entity_id(file, from),
                            &to,
                            TIER_EXTRACTED,
                        )]
                    }
                })
                .collect::<Vec<_>>();
            (
                (file, source, mentions, common::fixtures::landed(&graph)),
                (
                    file,
                    source,
                    Some(
                        written
                            .iter()
                            .map(|(name, leaf, _)| (name.to_string(), leaf.to_string()))
                            .collect(),
                    ),
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
    graph.mentions(file).map(|mentions| {
        let mut mentions: Vec<_> = (mentions.into_iter())
            .map(|mention| (mention.name, mention.leaf))
            .collect();
        mentions.sort();
        mentions
    })
}

/// One Rust file `f.rs` per call shape RUST WRITES THE QUALIFIER names (spec 117, RUST
/// REFERENCES), one module-prefixed turbofish, whose type the rule reads by its final segment, and
/// one per shape AN EXCLUDED QUALIFIER STAGES NOTHING names - a standard-library import's
/// qualifier, in a call and in a macro's token tree, and an alias qualifier - each `(what it
/// shows, source, its stored mentions as (name as written, leaf), the calls it lands as (caller,
/// callee), and the `impl` mentions it lands as (impl, self type) `REFERENCES` edges, each by id
/// path and `extracted`)`. An impl header stages an `impl` mention of its self type (spec 117,
/// RUST TAGS IMPL HEADERS), so `impl Store` referencing `Store` is a row's landing too.
#[cfg(feature = "symbols")]
type RustQualifiedCall = (
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
    &'static [(&'static str, &'static str)],
    &'static [(&'static str, &'static str)],
);

/// The rows of [`graph_build_names_a_rust_call_by_the_qualifier_rust_writes`].
#[cfg(feature = "symbols")]
const RUST_QUALIFIED_CALLS: [RustQualifiedCall; 9] = [
    (
        "`Store::open()`, its uppercase segment naming no type of the tree, makes no link",
        "fn open() {}\nfn main() { Store::open(); }\n",
        &[("Store", "Store"), ("Store::open", "open")],
        &[],
        &[],
    ),
    (
        "`a::Memo::<u8>::new()`, its turbofish type's final segment naming no type, makes no link",
        "fn new() {}\nfn main() { a::Memo::<u8>::new(); }\n",
        &[("Memo", "Memo"), ("Memo::new", "new")],
        &[],
        &[],
    ),
    (
        "`Self::new()` inside `impl<T> Store<T>` is qualified by the self type, generics stripped",
        "struct Store<T>(T);\nimpl<T> Store<T> { fn new() {} fn make() { Self::new() } }\n",
        &[("Store", "Store"), ("Store::new", "new")],
        &[("Store<T>::make", "Store<T>::new")],
        &[("impl Store<T>", "Store")],
    ),
    (
        "`self.file()` inside `trait CodeView` is qualified by the trait, over the free `file`",
        "trait CodeView { fn file(&self) {} fn lines(&self) { self.file() } }\nfn file() {}\n",
        &[("CodeView.file", "file")],
        &[("CodeView::lines", "CodeView::file")],
        &[],
    ),
    (
        "`self.inner.open()`, its receiver no bare `self`, lands on its one same-file `open`",
        "struct Inner;\nimpl Inner { fn open(&self) {} }\nstruct Store;\nimpl Store { fn reopen(&self) { self.inner.open() } }\n",
        &[
            ("Inner", "Inner"),
            ("Store", "Store"),
            ("Store.inner", "inner"),
            ("open", "open"),
        ],
        &[("Store::reopen", "Inner::open")],
        &[("impl Inner", "Inner"), ("impl Store", "Store")],
    ),
    (
        "`paths::module_dir()`, its segment lowercase, lands on its one same-file `module_dir`",
        "fn module_dir() {}\nfn main() { paths::module_dir(); }\n",
        &[("module_dir", "module_dir")],
        &[("main", "module_dir")],
        &[],
    ),
    (
        "`Cursor::new(x)` under `use std::io::Cursor;`, its qualifier a standard-library import, \
         is stored as no mention and lands nowhere, where a bare `new` lands on its same-file `new`",
        "use std::io::Cursor;\nfn new() {}\nfn a(x: u8) { Cursor::new(x); }\n",
        &[],
        &[],
        &[],
    ),
    (
        "`Cursor::new(v)` in a token tree under `use std::io::Cursor;` is stored as no mention, \
         the macro's own call landing on its one same-file macro",
        "use std::io::Cursor;\nmacro_rules! check { ($($t:tt)*) => {}; }\nfn new() {}\nfn b(c: u8, v: u8) { check!(c, Cursor::new(v)); }\n",
        &[("check", "check")],
        &[("b", "check")],
        &[],
    ),
    (
        "`C::f()` under `use a::B as C;` is stored as `B::f`, the name the alias imports, and \
         makes no link, its qualifier naming no type of the tree",
        "use a::B as C;\nfn f() {}\nfn g() { C::f(); }\n",
        &[("B", "B"), ("B::f", "f"), ("C", "C")],
        &[],
        &[],
    ),
];

/// GIVEN a project whose one Rust file `f.rs` makes a call in a shape RUST WRITES THE QUALIFIER
/// or AN EXCLUDED QUALIFIER STAGES NOTHING names, or through a module-prefixed turbofish
/// ([`RUST_QUALIFIED_CALLS`]),
/// WHEN the operator runs `rigger graph build`,
/// THEN the store holds the call's mention named as the Rust qualifier rule writes it -
/// `Store::open`, `Memo::new` and `Store::new` as paths, `B::f` for `C::f()` under
/// `use a::B as C;`, `CodeView.file` as a member access, `open` and `module_dir` bare - beside the
/// mention a `use`, an uppercase path segment, an impl header's self type or a field read
/// (`self.inner`, stored `Store.inner`, which no field of the tree answers) stages, and none for
/// `Cursor::new` under `use std::io::Cursor;`, in a call or in a macro's token tree, its qualifier
/// a standard-library import; and the graph's `REFERENCES` and `CALLS` edges are exactly where
/// those names reach: nowhere for `Store::open`, `Memo::new` and `B::f`, their qualifiers naming
/// no type of the tree, nor for the unstored `Cursor::new`, where a bare `open`, `new` or `f`
/// lands on its same-file fn; the trait's own `file` for `CodeView.file`, where a bare `file`
/// lands on a placeholder beside the same-file free `file`; and the one same-file candidate for
/// each other name - the macro's own `check` among them - which `paths::module_dir`, its
/// qualifier naming no type, would not reach.
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
        .map(|(shows, source, mentions, calls, impls)| {
            let (_project, graph) = common::cli::built_graph(&[("f.rs", source)]);
            let id = |path| entity_id("f.rs", path);
            let mut edges: Vec<_> = (calls.iter())
                .flat_map(|(caller, callee)| {
                    common::fixtures::a_call_from("f.rs", caller, &id(callee), TIER_EXTRACTED)
                })
                .chain((impls.iter()).map(|(from, to)| {
                    common::fixtures::reference(&id(from), &id(to), TIER_EXTRACTED)
                }))
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
        "each Rust call is stored named by the qualifier Rust writes, or not at all when that \
         qualifier is excluded, and lands where that name reaches"
    );
}

/// A Rust file holding the three field uses RUST STAGES FIELD USES names (spec 117) beside the
/// fields they name: a struct `SpawnOpts` with a field `system_prompt` (line 1) and a method `run`
/// (line 2), a struct `Store` with a field `root` (line 3) whose method `at` reads `self.root`
/// (line 4), a `fn a` assigning `opts.system_prompt` and calling `opts.run()` (line 5), and a
/// `fn b` writing the struct literal `SpawnOpts { system_prompt }` (line 6). Each field use sits
/// in its own fn, and no method shares a field's name.
#[cfg(feature = "symbols")]
const FIELD_USES_SOURCE: &str = "struct SpawnOpts { system_prompt: u8 }\nimpl SpawnOpts { fn run(&self) {} }\nstruct Store { root: u8 }\nimpl Store { fn at(&self) -> u8 { self.root } }\nfn a(mut opts: SpawnOpts, x: u8) { opts.system_prompt = x; opts.run(); }\nfn b(system_prompt: u8) { SpawnOpts { system_prompt }; }\n";

/// A Rust file holding the variant uses RUST STAGES VARIANT USES names (spec 117) beside the
/// enums that declare them: `InstallOutcome` with the unit variant `AlreadyCurrent` (line 1),
/// `Located` with the tuple variant `Many` (line 2), `Kind` with the struct variant
/// `RejectRecurrence { n }` (line 3) and `Hop` with the tuple variant `Frontier` (line 4), a trait
/// `Tr` whose `get` returns `Self::Target` (line 5), and one fn per use: `a` reading
/// `InstallOutcome::AlreadyCurrent` (line 6), `b` matching `Located::Many(c) =>` (line 7), `c`
/// matching `Kind::RejectRecurrence { n } =>` (line 8) and `e` calling `Hop::Frontier(cs)` (line
/// 9). None of `a`, `b`, `c` and `e` takes or returns a type of the file, so each of their type
/// mentions is its path's.
#[cfg(feature = "symbols")]
const VARIANT_USES_SOURCE: &str = "enum InstallOutcome { AlreadyCurrent }\nenum Located { Many(u8) }\nenum Kind { RejectRecurrence { n: u8 } }\nenum Hop { Frontier(u8) }\ntrait Tr { type Target; fn get(&self) -> Self::Target; }\nfn a() { let o = InstallOutcome::AlreadyCurrent; }\nfn b(x: u8) -> u8 { match x { Located::Many(c) => c, _ => 0 } }\nfn c(k: u8) -> u8 { match k { Kind::RejectRecurrence { n } => n, _ => 0 } }\nfn e(cs: u8) { Hop::Frontier(cs); }\n";

/// A Rust file holding the impl headers RUST TAGS IMPL HEADERS names (spec 117) beside what they
/// name: a trait `CodeView` with the method `file` (line 1), a struct `FactsView` (line 2), the
/// trait impl `impl<T> CodeView for FactsView<T>` (line 3), the inherent `impl FactsView` with
/// the method `new` (line 4), a free `new` (line 5) and `file` (line 6), a `fn a` calling
/// `FactsView::new()` and `FactsView::file()` (line 7), and a `fn host` holding a struct `Local`
/// and its trait impl `impl CodeView for Local` (line 8). Each call's leaf names two same-file
/// functions, so only its qualifier can narrow it to one.
#[cfg(feature = "symbols")]
const IMPL_HEADERS_SOURCE: &str = "trait CodeView { fn file(&self) {} }\nstruct FactsView;\nimpl<T> CodeView for FactsView<T> {}\nimpl FactsView { fn new() {} }\nfn new() {}\nfn file() {}\nfn a() { FactsView::new(); FactsView::file(); }\nfn host() { struct Local; impl CodeView for Local {} }\n";

/// A Rust file holding impl headers whose self type sits behind a reference or a pointer
/// (spec 117, RUST TAGS IMPL HEADERS) beside project types sharing those types' names: a
/// `use std::fmt` (line 1), a trait `Draw` (line 2), the structs `Path` (line 3) and `Formatter`
/// (line 4), and the trait impls `impl Draw for &std::path::Path` (line 5),
/// `impl Draw for *const std::path::Path` (line 6), `impl Draw for &fmt::Formatter` (line 7) and
/// `impl Draw for &crate::Path` (line 8). The first three self types are rooted at the standard
/// library or a standard-library import, the last at the project.
#[cfg(feature = "symbols")]
const IMPL_HEADERS_BEHIND_A_POINTER_SOURCE: &str = "use std::fmt;\ntrait Draw {}\nstruct Path;\nstruct Formatter;\nimpl Draw for &std::path::Path {}\nimpl Draw for *const std::path::Path {}\nimpl Draw for &fmt::Formatter {}\nimpl Draw for &crate::Path {}\n";

/// GIVEN a project whose one Rust file holds a row's uses - the field uses of
/// [`FIELD_USES_SOURCE`], the variant uses of [`VARIANT_USES_SOURCE`] or the impl headers of
/// [`IMPL_HEADERS_SOURCE`] or [`IMPL_HEADERS_BEHIND_A_POINTER_SOURCE`] -
/// WHEN the operator runs `rigger graph build`,
/// THEN the code store holds exactly the row's mentions, each named as the Rust qualifier rule
/// writes it, and the graph's `REFERENCES` and `CALLS` edges are exactly the row's, each landing
/// where its name reaches, with the row's definitions as its code entities and no placeholder
/// standing for any mention.
///
/// The field row: `system_prompt` unqualified, `SpawnOpts::system_prompt` as a path and
/// `Store.root` as a member access are `field` mentions and `opts.run()` a call alone, beside the
/// type mentions of `SpawnOpts` and the impl headers' `impl` mentions of their self types, each
/// landing on its type `extracted` from its impl (spec 117, RUST TAGS IMPL HEADERS); the
/// unqualified `system_prompt` lands on the one field of that name `inferred` though that field is
/// in its own file, as an unqualified field use never prefers its file;
/// `SpawnOpts::system_prompt` and `Store.root` land on it and on `Store`'s `root` `extracted`,
/// each qualifier narrowing its candidates to the one field its struct holds; and `opts.run()` as
/// the call's two edges onto `SpawnOpts::run`.
///
/// The variant row: `InstallOutcome::AlreadyCurrent` and `Located::Many(c) =>` are `variant`
/// mentions named by their enum beside the `type` mention of that enum; `Kind::RejectRecurrence
/// { n } =>` is the `variant` mention `Kind::RejectRecurrence` and the `field` mention
/// `RejectRecurrence::n`, with no `type` mention of `Kind`; `Hop::Frontier(cs)` is the call
/// `Hop::Frontier` beside the `type` mention of `Hop`; and `-> Self::Target` is the `type` mention
/// `Target` alone. Each qualifier narrows its candidates to the one variant or field its enum or
/// variant holds, so every edge is `extracted`: each variant use onto its variant, the field onto
/// `RejectRecurrence`'s `n`, the call as the call's two edges onto the variant `Hop::Frontier`,
/// and each type mention onto its enum or associated type.
///
/// The impl-header row: `impl<T> CodeView for FactsView<T>` is the `impl` mentions `CodeView` and
/// `FactsView`, `impl FactsView` the `impl` mention `FactsView` and the fn-local
/// `impl CodeView for Local` the `impl` mentions `CodeView` and `Local`, each enclosed by its own
/// impl, the fn-local two by `host::impl CodeView for Local` and never `host`, and no header is a
/// call or a `type` mention (spec 117, RUST TAGS IMPL HEADERS). Each lands `extracted` from its
/// impl on the trait or type it names. The calls `FactsView::new()` and `FactsView::file()` read
/// those mentions: `FactsView`'s impls are the impls enclosing an `impl` mention of it and the
/// traits they implement are the ones their `impl` mentions name, so `FactsView::new()` lands on
/// the `new` that `impl FactsView` defines and `FactsView::file()` on the trait's `file`, each
/// `extracted` over its same-file namesake, beside the `type` mention of `FactsView` the calls'
/// qualifier makes.
///
/// The behind-a-pointer row: each header's trait is the `impl` mention `Draw`, landing `extracted`
/// from its impl on the trait. A self type behind `&` or `*const` is read by its base name and
/// excluded by that name's path root, so `&std::path::Path`, `*const std::path::Path` and
/// `&fmt::Formatter` are no mention and land nothing on the project's `Path` or `Formatter`, while
/// `&crate::Path` is the `impl` mention `Path`, landing `extracted` on the project's `Path`.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_lands_each_rust_field_variant_and_impl_header_mention_on_its_definition_and_a_call_as_a_call(
) {
    use crate::symbol_index::mention_in;
    use rigger::contextgraph::query::entity_id;
    use rigger::contextgraph::{
        REF_KIND_FIELD, REF_KIND_IMPL, REF_KIND_TYPE, REF_KIND_VARIANT, TIER_EXTRACTED,
        TIER_INFERRED,
    };

    let file = "f.rs";
    let call_in = |at, name, leaf, line| mention_in(Some(at), name, leaf, None, line, false);
    let tagged_in =
        |at, name, leaf, kind, line| mention_in(Some(at), name, leaf, Some(kind), line, false);
    let id = |path| entity_id(file, path);
    // Each row: its source, the mentions the store holds for it, the `(caller, to)` of each call
    // landed `extracted`, the `(from, to, tier)` of each other `REFERENCES` edge, its code
    // entities, and why. A mention sits inside the definition at index `at`.
    let cases = [
        (
            FIELD_USES_SOURCE,
            // `Store::at` is 5, `a` 6, `b` 7, and the impls of `SpawnOpts` and `Store` 8 and 9.
            vec![
                tagged_in(5, "Store.root", "root", REF_KIND_FIELD, 4),
                tagged_in(6, "system_prompt", "system_prompt", REF_KIND_FIELD, 5),
                call_in(6, "run", "run", 5),
                tagged_in(
                    7,
                    "SpawnOpts::system_prompt",
                    "system_prompt",
                    REF_KIND_FIELD,
                    6,
                ),
                tagged_in(6, "SpawnOpts", "SpawnOpts", REF_KIND_TYPE, 5),
                tagged_in(7, "SpawnOpts", "SpawnOpts", REF_KIND_TYPE, 6),
                tagged_in(8, "SpawnOpts", "SpawnOpts", REF_KIND_IMPL, 2),
                tagged_in(9, "Store", "Store", REF_KIND_IMPL, 4),
            ],
            vec![("a", "SpawnOpts::run")],
            vec![
                ("impl SpawnOpts", "SpawnOpts", TIER_EXTRACTED),
                ("impl Store", "Store", TIER_EXTRACTED),
                ("a", "SpawnOpts.system_prompt", TIER_INFERRED),
                ("a", "SpawnOpts", TIER_EXTRACTED),
                ("b", "SpawnOpts.system_prompt", TIER_EXTRACTED),
                ("b", "SpawnOpts", TIER_EXTRACTED),
                ("Store::at", "Store.root", TIER_EXTRACTED),
            ],
            vec![
                "SpawnOpts",
                "SpawnOpts.system_prompt",
                "SpawnOpts::run",
                "Store",
                "Store.root",
                "Store::at",
                "a",
                "b",
                "impl SpawnOpts",
                "impl Store",
            ],
            "each field use is stored a field mention named as Rust writes its qualifier and \
             `opts.run()` a call; each lands where its name reaches, the unqualified one inferred, \
             the qualified two extracted, the call as a call; and no placeholder stands for any \
             mention",
        ),
        (
            VARIANT_USES_SOURCE,
            // `Tr` is 9, `a` 12, `b` 13, `c` 14 and `e` 15.
            vec![
                tagged_in(14, "RejectRecurrence::n", "n", REF_KIND_FIELD, 8),
                call_in(15, "Hop::Frontier", "Frontier", 9),
                tagged_in(9, "Target", "Target", REF_KIND_TYPE, 5),
                tagged_in(12, "InstallOutcome", "InstallOutcome", REF_KIND_TYPE, 6),
                tagged_in(
                    12,
                    "InstallOutcome::AlreadyCurrent",
                    "AlreadyCurrent",
                    REF_KIND_VARIANT,
                    6,
                ),
                tagged_in(13, "Located", "Located", REF_KIND_TYPE, 7),
                tagged_in(13, "Located::Many", "Many", REF_KIND_VARIANT, 7),
                tagged_in(
                    14,
                    "Kind::RejectRecurrence",
                    "RejectRecurrence",
                    REF_KIND_VARIANT,
                    8,
                ),
                tagged_in(15, "Hop", "Hop", REF_KIND_TYPE, 9),
            ],
            vec![("e", "Hop::Frontier")],
            vec![
                ("Tr", "Tr::Target", TIER_EXTRACTED),
                ("a", "InstallOutcome", TIER_EXTRACTED),
                ("a", "InstallOutcome::AlreadyCurrent", TIER_EXTRACTED),
                ("b", "Located", TIER_EXTRACTED),
                ("b", "Located::Many", TIER_EXTRACTED),
                ("c", "Kind::RejectRecurrence", TIER_EXTRACTED),
                ("c", "Kind::RejectRecurrence.n", TIER_EXTRACTED),
                ("e", "Hop", TIER_EXTRACTED),
            ],
            vec![
                "Hop",
                "Hop::Frontier",
                "Hop::Frontier.0",
                "InstallOutcome",
                "InstallOutcome::AlreadyCurrent",
                "Kind",
                "Kind::RejectRecurrence",
                "Kind::RejectRecurrence.n",
                "Located",
                "Located::Many",
                "Located::Many.0",
                "Tr",
                "Tr::Target",
                "Tr::get",
                "a",
                "b",
                "c",
                "e",
            ],
            "each variant use is stored a variant mention named by its enum, a struct pattern's \
             field a field mention named by its variant with no type mention of its enum, a tuple \
             variant's construction a call and a type-position path a type mention alone; each \
             lands extracted on the one definition its qualifier narrows to, the call as a call; \
             and no placeholder stands for any mention",
        ),
        (
            IMPL_HEADERS_SOURCE,
            // `a` is 6, and the impls of `CodeView for FactsView<T>`, `FactsView` and
            // `CodeView for Local` 9, 10 and 11.
            vec![
                call_in(6, "FactsView::new", "new", 7),
                call_in(6, "FactsView::file", "file", 7),
                tagged_in(6, "FactsView", "FactsView", REF_KIND_TYPE, 7),
                tagged_in(9, "CodeView", "CodeView", REF_KIND_IMPL, 3),
                tagged_in(9, "FactsView", "FactsView", REF_KIND_IMPL, 3),
                tagged_in(10, "FactsView", "FactsView", REF_KIND_IMPL, 4),
                tagged_in(11, "CodeView", "CodeView", REF_KIND_IMPL, 8),
                tagged_in(11, "Local", "Local", REF_KIND_IMPL, 8),
            ],
            vec![("a", "FactsView::new"), ("a", "CodeView::file")],
            vec![
                ("a", "FactsView", TIER_EXTRACTED),
                ("impl CodeView for FactsView<T>", "CodeView", TIER_EXTRACTED),
                (
                    "impl CodeView for FactsView<T>",
                    "FactsView",
                    TIER_EXTRACTED,
                ),
                ("impl FactsView", "FactsView", TIER_EXTRACTED),
                ("host::impl CodeView for Local", "CodeView", TIER_EXTRACTED),
                (
                    "host::impl CodeView for Local",
                    "host::Local",
                    TIER_EXTRACTED,
                ),
            ],
            vec![
                "CodeView",
                "CodeView::file",
                "FactsView",
                "FactsView::new",
                "a",
                "file",
                "host",
                "host::Local",
                "host::impl CodeView for Local",
                "impl CodeView for FactsView<T>",
                "impl FactsView",
                "new",
            ],
            "each impl header's trait and self type is stored an impl mention its impl encloses, \
             a function-local impl's never its function, and no call or type mention; each lands \
             extracted from its impl on what it names; and through them the type-qualified calls \
             land extracted on the impl's method and the trait's method, no placeholder standing \
             for any mention",
        ),
        (
            IMPL_HEADERS_BEHIND_A_POINTER_SOURCE,
            // The impls of `&std::path::Path`, `*const std::path::Path`, `&fmt::Formatter` and
            // `&crate::Path` are 3, 4, 5 and 6.
            vec![
                tagged_in(3, "Draw", "Draw", REF_KIND_IMPL, 5),
                tagged_in(4, "Draw", "Draw", REF_KIND_IMPL, 6),
                tagged_in(5, "Draw", "Draw", REF_KIND_IMPL, 7),
                tagged_in(6, "Draw", "Draw", REF_KIND_IMPL, 8),
                tagged_in(6, "Path", "Path", REF_KIND_IMPL, 8),
            ],
            vec![],
            vec![
                ("impl Draw for &std::path::Path", "Draw", TIER_EXTRACTED),
                (
                    "impl Draw for *const std::path::Path",
                    "Draw",
                    TIER_EXTRACTED,
                ),
                ("impl Draw for &fmt::Formatter", "Draw", TIER_EXTRACTED),
                ("impl Draw for &crate::Path", "Draw", TIER_EXTRACTED),
                ("impl Draw for &crate::Path", "Path", TIER_EXTRACTED),
            ],
            vec![
                "Draw",
                "Formatter",
                "Path",
                "impl Draw for &crate::Path",
                "impl Draw for &fmt::Formatter",
                "impl Draw for &std::path::Path",
                "impl Draw for *const std::path::Path",
            ],
            "a self type behind a reference or a pointer is stored an impl mention by its base \
             name unless that name's path is rooted at the standard library or a standard-library \
             import, so only the crate-rooted one lands on the project's same-named type, and \
             every header's trait lands extracted from its impl",
        ),
    ];
    for (source, mentions, calls, references, entities, why) in cases {
        let (_project, graph) = common::cli::built_graph(&[(file, source)]);
        let mut edges: Vec<_> = (calls.iter())
            .flat_map(|(caller, to)| {
                common::fixtures::a_call_from(file, caller, &id(to), TIER_EXTRACTED)
            })
            .collect();
        edges.extend(
            (references.iter())
                .map(|(from, to, tier)| common::fixtures::reference(&id(from), &id(to), tier)),
        );
        edges.sort();
        assert_eq!(
            (
                graph.mentions(file),
                common::fixtures::landed(&graph),
                common::fixtures::live_file_and_entity_ids(&graph.whole().unwrap()),
            ),
            (
                Some(mentions),
                edges,
                [
                    vec![file.to_string()],
                    entities.into_iter().map(id).collect()
                ],
            ),
            "{why}"
        );
    }
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
