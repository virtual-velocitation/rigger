//! Periphery (CLI) tests for `rigger graph communities [--resolution <r>]` over the built binary:
//! a read-only report of the communities the code store holds for the head. The membership
//! itself is proven over the library (`community_detection_pass.rs`); this file guards the
//! subcommand's dispatch, its argument parsing, its summary and that it records nothing.

#[cfg(feature = "symbols")]
use crate::common::fixtures::two_subsystem_sources;
use crate::layer_cli::LayerCli;

/// The subcommand under test, grouping the two-subsystem sources.
pub(crate) const COMMUNITIES: LayerCli = LayerCli {
    subcommand: "communities",
    empty_summary: "graph communities: 0 communities over 0 node(s) at resolution 1",
    #[cfg(feature = "symbols")]
    tree: two_subsystem_sources,
};

#[cfg(feature = "symbols")]
#[test]
fn the_subcommand_reports_the_communities_of_the_head() {
    let dir = COMMUNITIES.project(&two_subsystem_sources());
    let (summary, clusters) = COMMUNITIES.report(dir.path(), &[]);
    assert!(
        summary.starts_with(&format!(
            "graph communities: {} communities over ",
            clusters.len()
        )),
        "the summary counts the reported communities: {summary}"
    );
    assert!(
        clusters.len() >= 2,
        "at least the two subsystems are communities: {clusters:?}"
    );
}

rigger::test_cases! {
    an_empty_project_reports_no_community_and_still_succeeds:
        COMMUNITIES.an_empty_project_reports_no_grouping_and_still_succeeds();
    a_malformed_resolution_or_unknown_argument_fails_loudly:
        COMMUNITIES.a_malformed_resolution_or_unknown_argument_fails_loudly();
    #[cfg(feature = "symbols")]
    the_report_writes_nothing_to_the_log:
        COMMUNITIES.the_report_writes_nothing_to_the_log();
    #[cfg(feature = "symbols")]
    each_grain_names_its_communities_by_its_resolution:
        COMMUNITIES.each_grain_names_its_clusters_by_its_resolution();
}
