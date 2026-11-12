//! Periphery proof for the `--resolution` knob over a real tree: the groupings computed at a grain
//! over the code store's view of the two-subsystem sources refine as the resolution rises, and each
//! grain names its communities by its resolution.

use std::collections::BTreeSet;

use rigger_domain::code_view::Cluster;
use rigger_domain::groupings::group_at;

use crate::community_detection_pass::two_subsystem_view;

/// The partition `clusters` draws over their members.
fn partition(clusters: &[Cluster]) -> BTreeSet<BTreeSet<String>> {
    clusters
        .iter()
        .map(|c| c.members.iter().cloned().collect())
        .collect()
}

#[test]
fn a_higher_resolution_yields_at_least_as_many_communities() {
    let view = two_subsystem_view();
    let at = |r: f64| group_at(&view, r).communities;

    let sweep = [0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 6.0];
    let counts: Vec<usize> = sweep.iter().map(|&r| at(r).len()).collect();
    for w in counts.windows(2) {
        assert!(
            w[1] >= w[0],
            "a higher resolution never yields fewer communities; sweep {sweep:?} -> {counts:?}"
        );
    }
    let (coarse, fine) = (counts[0], counts[counts.len() - 1]);
    assert!(
        fine > coarse,
        "a higher resolution refines the grain; sweep {sweep:?} -> {counts:?}"
    );

    let maxed = at(50.0);
    let nodes: usize = maxed.iter().map(|c| c.members.len()).sum();
    assert!(
        nodes >= 8,
        "the tree couples a non-trivial graph, got {nodes}"
    );
    assert_eq!(
        maxed.len(),
        nodes,
        "a high enough resolution isolates every one of the {nodes} coupled nodes"
    );

    let coarsest = at(sweep[0]);
    assert_ne!(
        partition(&coarsest),
        partition(&maxed),
        "distinct resolutions group the same nodes differently"
    );
    for (clusters, prefix) in [(&coarsest, "community/0.5/"), (&maxed, "community/50/")] {
        assert!(
            clusters.iter().all(|c| c.id.starts_with(prefix)),
            "a grain's community ids carry its resolution {prefix}: {clusters:?}"
        );
    }
}
