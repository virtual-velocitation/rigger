//! Periphery (CLI) tests for `rigger graph concepts [--resolution <r>]` over the built binary: a
//! read-only report of the concepts the code store holds for the head. The derivation itself is
//! proven over the library (`concepts_shared_assembly.rs`, `rigger_domain::groupings`); this file
//! guards the subcommand's dispatch, its argument parsing, its summary and that it records nothing.

use crate::layer_cli::LayerCli;

#[cfg(feature = "symbols")]
/// A tree of two design documents, each specifying the code of its own region across two
/// directories.
fn intent_tree() -> Vec<(String, String)> {
    [
        (
            "docs/graph-design.md",
            "# The knowledge graph\n\nThe code is `src/graph/store.rs` and `src/db/sqlite.rs`.\n",
        ),
        (
            "docs/review-design.md",
            "# Review adjudication\n\nThe code is `src/review/panel.rs` and `src/verdict/judge.rs`.\n",
        ),
        ("src/graph/store.rs", "pub fn store() {}\n"),
        ("src/db/sqlite.rs", "pub fn sqlite() {}\n"),
        ("src/review/panel.rs", "pub fn panel() {}\n"),
        ("src/verdict/judge.rs", "pub fn judge() {}\n"),
    ]
    .map(|(path, text)| (path.to_string(), text.to_string()))
    .to_vec()
}

/// The subcommand under test, grouping the two design regions.
const CONCEPTS: LayerCli = LayerCli {
    subcommand: "concepts",
    empty_summary: "graph concepts: 0 concepts over 0 node(s) at resolution 1",
    #[cfg(feature = "symbols")]
    tree: intent_tree,
};

#[cfg(feature = "symbols")]
#[test]
fn the_subcommand_reports_the_concepts_of_the_head() {
    let dir = CONCEPTS.project(&intent_tree());
    let (summary, clusters) = CONCEPTS.report(dir.path(), &[]);
    assert!(
        summary.starts_with(&format!(
            "graph concepts: {} concepts over ",
            clusters.len()
        )),
        "the summary counts the reported concepts: {summary}"
    );
    assert_eq!(
        clusters.len(),
        2,
        "each design region is one concept: {clusters:?}"
    );
    assert!(
        clusters.iter().all(|(_, members, _)| *members == 3),
        "each concept groups its document with the code it specifies: {clusters:?}"
    );
}

rigger::test_cases! {
    an_empty_project_reports_no_concept_and_still_succeeds:
        CONCEPTS.an_empty_project_reports_no_grouping_and_still_succeeds();
    a_malformed_resolution_or_unknown_argument_fails_loudly:
        CONCEPTS.a_malformed_resolution_or_unknown_argument_fails_loudly();
    #[cfg(feature = "symbols")]
    the_report_writes_nothing_to_the_log:
        CONCEPTS.the_report_writes_nothing_to_the_log();
    #[cfg(feature = "symbols")]
    each_grain_names_its_concepts_by_its_resolution:
        CONCEPTS.each_grain_names_its_clusters_by_its_resolution();
}
