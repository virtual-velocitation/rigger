//! Fixtures for the dash's lens suites: the lens under test and the graphs it folds.

use rigger::contextgraph::{Graph, KIND_CODE_ENTITY, KIND_CONCEPT, REL_REALIZES, TIER_EXTRACTED};
use rigger::dash::{clustered_overview, Cluster, ClusterEdge, Lens};

use super::fixtures::{edge, node_with_attrs, node_with_optional_attrs};

/// Lens `name` (`code`, `concepts`, ...) at resolution grain `1`.
pub fn lens(name: &str) -> Lens {
    Lens::from_query(Some(name), Some("1"))
}

/// The concept subject of [`shared_member_graph`].
pub const SUB_C: &str = "concept/1/0";
/// The other, larger concept of [`shared_member_graph`].
pub const OTHER_D: &str = "concept/1/1";
/// The member of [`shared_member_graph`] that realizes BOTH concepts.
pub const SHARED_MEMBER: &str = "src/a.rs::m";
/// The member of [`shared_member_graph`] that realizes only [`SUB_C`].
pub const SOLO_MEMBER: &str = "src/b.rs::n";

/// Concept [`SUB_C`] has members `{m, n}`; `m` ALSO realizes [`OTHER_D`], which has two more
/// realizers (`p`, `q`) and so is the larger concept - `m`'s PRIMARY bucket.
pub fn shared_member_graph() -> Graph {
    let def = |id: &str, name: &str| node_with_attrs(id, KIND_CODE_ENTITY, &[("name", name)]);
    let realizes = |from: &str, to: &str| edge(from, to, REL_REALIZES, TIER_EXTRACTED);
    Graph {
        nodes: vec![
            node_with_optional_attrs(SUB_C, KIND_CONCEPT, &[("label", Some("cc"))]),
            node_with_optional_attrs(OTHER_D, KIND_CONCEPT, &[("label", Some("dd"))]),
            def(SHARED_MEMBER, "m"),
            def(SOLO_MEMBER, "n"),
            def("src/c.rs::p", "p"),
            def("src/d.rs::q", "q"),
        ],
        edges: vec![
            realizes(SHARED_MEMBER, SUB_C),
            realizes(SOLO_MEMBER, SUB_C),
            realizes(SHARED_MEMBER, OTHER_D),
            realizes("src/c.rs::p", OTHER_D),
            realizes("src/d.rs::q", OTHER_D),
        ],
    }
}

/// `lens`'s overview of `graph` over the public boundary: `total` still counts every graph node,
/// a derived grain carries no empty state, the fold yields exactly `clusters` and `edges`, and
/// none of `never_keys` (storage schema names the lens must never surface) is a cluster key.
pub fn assert_overview_folds(
    graph: &Graph,
    lens: &Lens,
    total: usize,
    clusters: Vec<Cluster>,
    edges: Vec<ClusterEdge>,
    never_keys: &[&str],
) {
    let overview = clustered_overview(graph, lens);
    assert_eq!(
        overview.total, total,
        "total carries every graph node, the excluded super-nodes included"
    );
    assert_eq!(
        overview.empty_state, None,
        "a DERIVED grain is not the empty state"
    );
    assert_eq!(
        overview.clusters, clusters,
        "the lens folds exactly its subjects (sized, dominant-kind, labelled) and excludes every \
         other node entirely: {overview:?}"
    );
    assert!(
        overview
            .clusters
            .iter()
            .all(|c| !never_keys.contains(&c.key.as_str())),
        "no storage-schema-name kind bucket ever appears as a cluster key: {overview:?}"
    );
    assert_eq!(
        overview.edges, edges,
        "only cross-bucket coupling weights the super-edge; intra-bucket edges and the spokes to \
         the excluded super-node add none: {overview:?}"
    );
}

/// `lens` at a resolution grain with NO derived assignments over `graph` folds nothing, still
/// reports all `total` graph nodes, and carries the documented `empty_state` prompt - never an
/// error and never a bare kind-bucket view.
pub fn assert_underived_grain_is_the_empty_state(
    graph: &Graph,
    lens: &Lens,
    total: usize,
    empty_state: &str,
) {
    let underived = clustered_overview(graph, lens);
    assert!(
        underived.clusters.is_empty() && underived.edges.is_empty(),
        "an underived grain folds no buckets: {underived:?}"
    );
    assert_eq!(
        underived.total, total,
        "the empty state still reports the whole graph size"
    );
    assert_eq!(
        underived.empty_state.as_deref(),
        Some(empty_state),
        "an underived grain carries the documented empty-state message, never an error"
    );
}
