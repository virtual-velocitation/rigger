//! Periphery (integration) tests for spec 117, TYPESCRIPT AND JAVASCRIPT STAGE THEIR ROWS, across
//! the seam the crate's own tests stop short of: the TypeScript, TSX and JavaScript registry
//! entries' queries tag each mention, the facts layer stores it, and the resolver lands it, all
//! inside the real `rigger graph build`. The crate's tests pin each staged mention in process;
//! this pins that the built code store holds it and that the built graph lands it on its one
//! definition: a TypeScript `IShape[]`, `implements IShape` and constant read, a JavaScript
//! `extends Base` and member read `x.depth`, and a `Square.make()` whose qualifier picks one of
//! two `make`s.
//!
//! NOT OWNED here: a `/** */` block's doc span (`doc_spans_reach_the_code_store_periphery`) and
//! a `new Ns.Box()` whose qualifier names nothing
//! (`references_are_extracted_and_isolated_periphery`).
//!
//! Every test extracts real source, so the file runs on the `symbols` lane alone.

use crate::common;

use rigger::code_facts::MentionKind;
use rigger::contextgraph::{
    REF_KIND_CONST, REF_KIND_FIELD, REF_KIND_IMPL, REF_KIND_TYPE, TIER_EXTRACTED, TIER_INFERRED,
};

/// One build of one file: what it shows, the file's root-relative path, its source, each stored
/// mention `(tag, name as written, leaf)` in source order with no tag for a call, the local path
/// of each definition the graph holds, each `REFERENCES` edge `(from, to, tier)` and each call
/// `(caller, to, tier)` it lands, by local path.
type Landing = (
    &'static str,
    &'static str,
    &'static str,
    &'static [(Option<&'static str>, &'static str, &'static str)],
    &'static [&'static str],
    &'static [(&'static str, &'static str, &'static str)],
    &'static [(&'static str, &'static str, &'static str)],
);

/// Two classes each holding a static `make`, and a function calling `Square.make()`: the call's
/// leaf alone has two same-file candidates, so only its qualifier picks one.
const QUALIFIED_CALL_SOURCE: &str = "class Square {\n    static make() {}\n}\nclass Other {\n    \
                                     static make() {}\n}\nfunction f() {\n    Square.make();\n}\n";

/// The rows of [`graph_build_lands_typescript_and_javascript_mentions_on_their_one_definition`].
const LANDING_FILES: [Landing; 8] = [
    (
        "a TypeScript array of an interface is a type mention",
        "f.ts",
        "interface IShape {}\nfunction area(sides: IShape[]) {}\n",
        &[(Some(REF_KIND_TYPE), "IShape", "IShape")],
        &["IShape", "area"],
        &[("area", "IShape", TIER_EXTRACTED)],
        &[],
    ),
    (
        "a TypeScript `implements` is an impl mention",
        "f.ts",
        "interface IShape {}\nclass Square implements IShape {}\n",
        &[(Some(REF_KIND_IMPL), "IShape", "IShape")],
        &["IShape", "Square"],
        &[("Square", "IShape", TIER_EXTRACTED)],
        &[],
    ),
    (
        "a TypeScript constant read is a const mention",
        "f.ts",
        "const LIMIT = 3;\nfunction cap() {\n    return LIMIT;\n}\n",
        &[(Some(REF_KIND_CONST), "LIMIT", "LIMIT")],
        &["LIMIT", "cap"],
        &[("cap", "LIMIT", TIER_EXTRACTED)],
        &[],
    ),
    (
        "TSX stages the same impl, type and const mentions",
        "f.tsx",
        "interface IShape {}\nconst LIMIT = 3;\nclass View implements IShape {\n    \
         draw(sides: IShape[]) {\n        return LIMIT;\n    }\n}\n",
        &[
            (Some(REF_KIND_IMPL), "IShape", "IShape"),
            (Some(REF_KIND_TYPE), "IShape", "IShape"),
            (Some(REF_KIND_CONST), "LIMIT", "LIMIT"),
        ],
        &["IShape", "LIMIT", "View", "View::draw"],
        &[
            ("View", "IShape", TIER_EXTRACTED),
            ("View::draw", "IShape", TIER_EXTRACTED),
            ("View::draw", "LIMIT", TIER_EXTRACTED),
        ],
        &[],
    ),
    (
        "a JavaScript `extends` is an impl mention",
        "f.js",
        "class Base {}\nclass Square extends Base {}\n",
        &[(Some(REF_KIND_IMPL), "Base", "Base")],
        &["Base", "Square"],
        &[("Square", "Base", TIER_EXTRACTED)],
        &[],
    ),
    (
        "a JavaScript member read is a field mention, inferred onto the one field",
        "f.js",
        "class Shape {\n    depth = 2;\n}\nfunction total(x) {\n    return x.depth;\n}\n",
        &[(Some(REF_KIND_FIELD), "depth", "depth")],
        &["Shape", "Shape.depth", "total"],
        &[("total", "Shape.depth", TIER_INFERRED)],
        &[],
    ),
    (
        "a JavaScript `Square.make()` is a call named `Square.make`",
        "f.js",
        QUALIFIED_CALL_SOURCE,
        &[(None, "Square.make", "make")],
        &["Other", "Other::make", "Square", "Square::make", "f"],
        &[],
        &[("f", "Square::make", TIER_EXTRACTED)],
    ),
    (
        "a TypeScript `Square.make()` is a call named `Square.make`",
        "f.ts",
        QUALIFIED_CALL_SOURCE,
        &[(None, "Square.make", "make")],
        &["Other", "Other::make", "Square", "Square::make", "f"],
        &[],
        &[("f", "Square::make", TIER_EXTRACTED)],
    ),
];

/// GIVEN a project whose one file is a row of [`LANDING_FILES`], WHEN the operator runs
/// `rigger graph build`, THEN the code store holds exactly the row's mentions, each with its tag,
/// its name as written and its leaf, and the graph holds the file, the row's definitions and no
/// placeholder, with exactly the row's edges: each tagged mention a `REFERENCES` edge from its
/// enclosing definition onto its one same-file candidate, extracted, but a field mention inferred,
/// and the `Square.make()` call's two edges onto `Square::make`, never onto `Other::make`.
#[test]
fn graph_build_lands_typescript_and_javascript_mentions_on_their_one_definition() {
    use rigger::contextgraph::query::entity_id;
    use rigger_domain::code_view::CodeView;

    let (found, expected): (Vec<_>, Vec<_>) = LANDING_FILES
        .into_iter()
        .map(
            |(what, file, source, mentions, definitions, references, calls)| {
                let (_project, graph) = common::cli::built_graph(&[(file, source)]);
                let stored = graph.view.file(file).map(|facts| {
                    let mentions = facts.into_owned().mentions.into_iter();
                    let tag = |kind| match kind {
                        MentionKind::Call => None,
                        MentionKind::Tagged(tag) => Some(tag),
                    };
                    mentions
                        .map(|mention| (tag(mention.kind), mention.name, mention.leaf))
                        .collect::<Vec<_>>()
                });
                let found = (
                    what,
                    stored,
                    common::fixtures::landed(&graph),
                    common::fixtures::live_file_and_entity_ids(&graph.whole().unwrap()),
                );
                let id = |path: &str| entity_id(file, path);
                let mentions = mentions
                    .iter()
                    .map(|&(tag, name, leaf)| (tag.map(String::from), name.into(), leaf.into()))
                    .collect();
                let mut edges: Vec<_> = (references.iter())
                    .map(|&(from, to, tier)| common::fixtures::reference(&id(from), &id(to), tier))
                    .chain(calls.iter().flat_map(|&(caller, to, tier)| {
                        common::fixtures::a_call_from(file, caller, &id(to), tier)
                    }))
                    .collect();
                edges.sort();
                let ids = definitions.iter().map(|&path| id(path)).collect();
                (
                    found,
                    (what, Some(mentions), edges, [vec![file.to_string()], ids]),
                )
            },
        )
        .unzip();
    assert_eq!(
        found, expected,
        "each mention is stored with its tag, name and leaf, and lands on its one definition"
    );
}
