//! Periphery (API / contract / integration) tests for spec 117, REFERENCES ARE EXTRACTED AND
//! ISOLATED: a Rust const read, type reference and `use` are extracted tagged with their
//! `ref_kind`, and no consumer that counts calls - the fold's `REFERENCES` and `CALLS` edges among
//! them - reads one. The crate's own tests pin the extraction, the emit pass, the grounder's
//! counts and the fold arm one module at a time over hand-built input; these run OUTSIDE the
//! crates and guard the two boundaries a tagged reference crosses on its way to an operator:
//!
//!  - the graph `rigger graph build` folds: a real tree's tagged references fold no placeholder
//!    and no edge of the event arm's, each landing instead as the staged row's one edge onto its
//!    one definition (spec 117, STAGED REFERENCES), while its call folds as before; that edge
//!    lands only while one candidate of its reference's kind answers it - the same file's before
//!    the project's, `extracted` - retires with its definition and lands again on its return, and
//!    its file, never its fn, is listed referenced but not called; a file
//!    rewritten to a `use` that nothing defines keeps its file node and loses every edge, and a
//!    new `use`-only file gains its file node; and the tagged
//!    references of a test file, of a `#[cfg(test)]` module or under their own `#[cfg(test)]`
//!    fold no edge and prove no definition, while their call proves one;
//!  - the persisted symbol index: `SymRef.ref_kind` is written for a tagged reference and omitted
//!    for a call, the form an index saved before the field existed holds, and reads back so a
//!    reloaded index counts no tagged reference as a call.

mod common;
#[path = "common/symbol_index.rs"]
mod symbol_index;

#[cfg(feature = "symbols")]
use common::fixtures::{REFERENCE_KINDS_DEFINER as DEFINER, REFERENCE_KINDS_USER as USER};
use rigger::contextgraph::REF_KIND_TYPE;
use rigger::grounder::symbols::model::{FileSymbols, Lang, SymRef};
use symbol_index::{defining_f, g_called_in_f, SavedIndex};

/// A Rust file holding one `use` and nothing else: its whole extraction is one tagged reference.
#[cfg(feature = "symbols")]
const USE_ONLY_SOURCE: &str = "use crate::store::Ledger;\n";

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
    landed.extend(
        references
            .iter()
            .map(|(from, to, tier)| common::fixtures::reference(from, to, tier)),
    );
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
        [DEFINER_ENTITIES[0], DEFINER_ENTITIES[1]]
            .map(|seed| common::fixtures::referenced_not_called_ids(&graph, seed)),
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
        common::fixtures::referenced_not_called_ids(&removed, DEFINER_ENTITIES[1]),
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
/// `QUOTA_CAP`, and the production ingest emits no `ref_kind` event for any of them.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_proves_a_definition_by_test_code_s_call_alone() {
    let [definer, (_, user_source)] = common::fixtures::REFERENCE_KINDS_TREE;
    for test_code in [
        ("tests/kestrel.rs", user_source),
        ("src/probe.rs", TEST_REGION_SOURCE),
        ("src/gated.rs", TEST_ATTRIBUTED_SOURCE),
    ] {
        let (project, graph) = common::cli::built_graph(&[definer, test_code]);

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
        let edges =
            common::project_payloads(project.path(), rigger::contextgraph::TYPE_EDGE_INFERRED);
        assert_eq!(
            common::fixtures::tagged_payloads(&edges),
            [],
            "{}: test code emits no ref_kind event",
            test_code.0
        );
    }
}

/// A Rust file defining `f`, whose body calls `G` and names the type `Kestrel` on line 2.
fn a_call_beside_a_type_reference() -> FileSymbols {
    defining_f(vec![
        g_called_in_f(),
        SymRef {
            ref_kind: Some(REF_KIND_TYPE.into()),
            name: "Kestrel".into(),
            ..g_called_in_f()
        },
    ])
}

/// The persisted index writes `ref_kind` for the type reference alone - the call is written
/// without the key, the form an index saved before the field existed holds - and reads the file
/// back whole, so the reloaded index counts the call and never the type reference.
#[test]
fn the_persisted_index_keeps_a_tagged_reference_out_of_the_reloaded_calls() {
    let file = "a.rs";
    let saved = SavedIndex::save(file, a_call_beside_a_type_reference());
    assert_eq!(
        saved.bytes.matches("\"ref_kind\"").count(),
        1,
        "only the tagged reference writes the key; got:\n{}",
        saved.bytes
    );
    let reloaded = saved.reload();
    assert_eq!(reloaded.files()[file], a_call_beside_a_type_reference());
    let calls: Vec<&str> = reloaded.files()[file]
        .calls()
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(calls, ["G"]);
    assert_eq!(
        ["G", "Kestrel"].map(|name| reloaded.reference_degree(name, Lang::Rust)),
        [1, 0]
    );
}

/// The index the public `build_index` extracts from the reference-kinds tree reads back from
/// the real persistence boundary whole, its tagged references with their kinds and callers.
#[cfg(feature = "symbols")]
#[test]
fn build_index_tags_survive_the_save_load_pipeline() {
    let dir = tempfile::tempdir().unwrap();
    common::fixtures::write_files(dir.path(), &common::fixtures::REFERENCE_KINDS_TREE);

    let (built, reloaded) = symbol_index::built_and_reloaded(dir.path().to_str().unwrap());

    assert_eq!(reloaded.files(), built.files());
    assert_eq!(
        symbol_index::tagged_refs(&reloaded.files()[USER]),
        common::fixtures::REFERENCE_KINDS_USER_TAGGED
    );
}
