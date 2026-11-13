//! Fixtures for the dash's lens suites: the lens under test, the graphs it folds and the rows a
//! fold is asserted by.

use rigger::contextgraph::{Graph, KIND_CODE_ENTITY, KIND_CONCEPT, REL_REALIZES, TIER_EXTRACTED};
use rigger::dash::{clustered_overview, reproject, Cluster, ClusterEdge, Lens};

use super::fixtures::{edge, node_with_attrs, node_with_optional_attrs};
use super::served::{served, served_json};

/// The files lens over `graph`, whole and re-grained to `subject`, whose `members` it asserts:
/// the `whole <key> <count>` of each whole-graph cluster and the `whole <from> -- <to> <weight>`
/// of each cluster edge, then the `concept <key> <count>` of each bucket of the subject's members
/// and the `concept <id> <- <candidates>` of each unresolved one.
pub fn files_lens_rows(graph: &Graph, subject: &str, members: usize) -> Vec<String> {
    let overview = clustered_overview(graph, &Lens::Files);
    let regrain = reproject(graph, subject, &Lens::Files);
    assert_eq!(regrain.total, members, "the concept has {members} members");
    let mut rows: Vec<String> = Vec::new();
    for c in &overview.clusters {
        rows.push(format!("whole {} {}", c.key, c.count));
    }
    for e in &overview.edges {
        rows.push(format!("whole {} -- {} {}", e.from, e.to, e.weight));
    }
    for c in &regrain.clusters {
        rows.push(format!("concept {} {}", c.key, c.count));
    }
    for u in &regrain.unresolved {
        rows.push(format!("concept {} <- {:?}", u.id, u.candidates));
    }
    rows
}

/// Lens `name` (`code`, `concepts`, ...) at resolution grain `1`.
pub fn lens(name: &str) -> Lens {
    Lens::from_query(Some(name), Some("1"))
}

/// The concept subject of [`shared_member_graph`].
pub const SUB_C: &str = "concept/1/0";
/// The served `/api/graph` route re-graining [`SUB_C`] by the files lens.
pub const SUB_C_FILES_ROUTE: &str = "/api/graph?seed=concept%2F1%2F0&lens=files";
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

/// Spellings of the number `1` a hand-written `resolution=` may carry, each grain `1` (`%2B` is `+`).
pub const ONE_SPELLINGS: [&str; 5] = ["1.0", "1.00", "01", "1e0", "%2B1"];

/// Lens `name` over `graph` reads grain `1` whatever spelling of the number `1` its `resolution=`
/// carries: the served overview, the drill of `cluster` and the re-projection of `subject` at each
/// of [`ONE_SPELLINGS`] are byte-identical to the same view at `resolution=1`, which folds `cluster`
/// (no empty state) and drills it to its members.
pub fn assert_each_spelling_of_one_reads_grain_one(
    graph: &Graph,
    name: &str,
    cluster: &str,
    subject: &str,
) {
    let views = |grain: &str| {
        let at = format!("/api/graph?lens={name}&resolution={grain}");
        [
            at.clone(),
            format!("{at}&cluster={}", cluster.replace('/', "%2F")),
            format!("{at}&seed={}", subject.replace('/', "%2F")),
        ]
    };
    let [overview, drill, reprojection] = views("1").map(|v| served_json(graph, &v));
    assert!(
        overview["empty_state"].is_null()
            && (overview["clusters"].as_array().into_iter().flatten())
                .any(|c| c["key"].as_str() == Some(cluster)),
        "grain 1 is derived and folds {cluster}: {overview}"
    );
    assert!(
        drill["nodes"].as_array().is_some_and(|n| !n.is_empty()),
        "grain 1 drills {cluster} to its members: {drill}"
    );
    assert!(
        reprojection["empty_state"].is_null()
            && reprojection["clusters"]
                .as_array()
                .is_some_and(|c| !c.is_empty()),
        "grain 1 re-grains {subject} into derived buckets: {reprojection}"
    );
    let at_one = views("1").map(|v| served(graph, &v).body);
    for spelling in ONE_SPELLINGS {
        for (view, one) in views(spelling).iter().zip(&at_one) {
            assert_eq!(
                String::from_utf8_lossy(&served(graph, view).body),
                String::from_utf8_lossy(one),
                "{view} names grain 1, so it serves what resolution=1 serves"
            );
        }
    }
}
