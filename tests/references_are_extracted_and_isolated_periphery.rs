//! Periphery (API / contract / integration) tests for spec 117, REFERENCES ARE EXTRACTED AND
//! ISOLATED: a Rust const read, type reference and `use` are extracted tagged with their
//! `ref_kind`, and no consumer that counts calls - the fold's `REFERENCES` and `CALLS` edges among
//! them - reads one. The crate's own tests pin the extraction, the emit pass, the grounder's
//! counts and the fold arm one module at a time over hand-built input; these run OUTSIDE the
//! crates and guard the two boundaries a tagged reference crosses on its way to an operator:
//!
//!  - the graph `rigger graph build` folds: a real tree's tagged references fold no edge and no
//!    placeholder, while its call folds as before; a file rewritten to a `use` alone keeps its
//!    file node and loses every edge, and a new `use`-only file gains its file node; and a test
//!    file's tagged references prove no definition, while its call does;
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

/// GIVEN a project whose `src/shapes.rs` defines `Kestrel`, `QUOTA_CAP` and `draw` and whose
/// `src/user.rs` imports `Kestrel` and, in `fn paint`, takes it as a parameter type, calls `draw`
/// and reads `QUOTA_CAP`,
/// WHEN the operator runs `rigger graph build`,
/// THEN the graph's `REFERENCES` and `CALLS` edges are the call's two alone, landed on its
/// placeholder at the tier a cross-file definition earns, and its code entities are the
/// definitions and that one placeholder: the `use`, the type reference and the const read fold
/// no edge and no node.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_folds_a_files_call_alone_never_its_use_type_reference_or_const_read() {
    use rigger::contextgraph::TIER_INFERRED;

    let (_project, graph) = common::cli::built_graph(&common::fixtures::REFERENCE_KINDS_TREE);

    assert_eq!(
        common::fixtures::landed(&graph),
        common::fixtures::a_call_from(USER, "paint", DRAW_PLACEHOLDER, TIER_INFERRED),
        "the call to `draw` is the one reference that lands"
    );
    assert_eq!(
        common::fixtures::live_file_and_entity_ids(&graph.whole().unwrap()),
        common::fixtures::file_and_entity_ids(
            &[DEFINER, USER],
            &[
                &DEFINER_ENTITIES[..],
                &[DRAW_PLACEHOLDER, "src/user.rs::paint"]
            ]
            .concat()
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
        "a `use` folds no edge"
    );
    assert_eq!(
        common::fixtures::live_file_and_entity_ids(&graph.whole().unwrap()),
        common::fixtures::file_and_entity_ids(&[LEDGER, DEFINER, USER], &DEFINER_ENTITIES),
        "each `use`-only file stands as its file node alone"
    );
}

/// GIVEN the reference-kinds tree beside a test file `tests/kestrel.rs` whose body is the user
/// file's: it imports `Kestrel` and, in `fn paint`, takes it as a parameter type, calls `draw`
/// and reads `QUOTA_CAP`,
/// WHEN the operator runs `rigger graph build`,
/// THEN `draw` alone carries the test file's evidence (`proven_by`): its `use`, type reference
/// and const read prove neither `Kestrel` nor `QUOTA_CAP`.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_proves_a_definition_by_a_test_files_call_alone() {
    let [definer, (_, user_source)] = common::fixtures::REFERENCE_KINDS_TREE;
    let (_project, graph) = common::cli::built_graph(&[definer, ("tests/kestrel.rs", user_source)]);

    assert_eq!(
        DEFINER_ENTITIES.map(|id| common::fixtures::proven_by_attr(&graph, id)),
        [None, None, Some("1".to_string())],
        "the test file's call to `draw` is its one proof"
    );
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
