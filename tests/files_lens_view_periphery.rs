//! Periphery (API / integration / contract) tests for spec 63's FILES LENS (criterion 3): the
//! default `Lens::Files` whole-graph fold, purified the same shape as the already-merged code lens
//! (criterion 1). Only [`KIND_CODE_ENTITY`] nodes fold, each keyed by its OWN FILE ([`file_of`]) -
//! never its directory, never its kind - so a cluster IS a file, sized by its CONTAINED-ENTITY count;
//! weighted cross-file coupling edges connect files that reference each other; and drilling ANY
//! cluster is unconditionally empty, because a file is this lens's atomic LEAF subject. Handoff
//! MECHANICS (a file's card, its listed top entities) are criterion 2's, not this one's - this
//! criterion owns only the purity of what renders as a NODE, HUB, or GROUP LABEL in this lens, at any
//! zoom: no code-entity node and no per-type storage-schema bucket ever does.
//!
//! These run OUTSIDE the crate, over the library's PUBLIC surface (`rigger::dash::{Lens,
//! clustered_overview, cluster_detail, neighborhood, route, ...}`), so they guard the exact
//! boundaries the inside-out unit test (`src/dash.rs mod tests`, which reaches the same functions via
//! `super::` and calls the folds in-process) is structurally blind to:
//!
//!  - PUBLIC REACHABILITY. The unit test proves the files-lens BEHAVIOUR but never that the
//!    changed-signature `clustered_overview` / `cluster_detail` and their carrier DTOs stay `pub` and
//!    reachable as `rigger::dash::...`. If any were accidentally crate-private, only a crate-external
//!    test fails to COMPILE - the inside-out test would stay green.
//!  - THE SERVED ROUTE END-TO-END. The unit test calls `clustered_overview` / `cluster_detail`
//!    directly, never through `route`'s lens-absent dispatch (the DEFAULT the browser hits on every
//!    plain `/api/graph` load). A regression that forgot to reach the purity gate on that path is
//!    invisible in-process; here it reddens, because this drives the exact body-builder `serve` ships.
//!  - THE SERIALIZED WIRE-SHAPE back-compat: `Cluster.label` / `ClusterOverview.empty_state` /
//!    `Neighborhood.truncated` are all `skip_serializing_if`, so the files overview an external JS
//!    panel reads carries NO `label`, NO `empty_state`, and a files drill carries NO `truncated` (it
//!    is always complete - trivially, vacuously - since it is always empty). The unit test asserts
//!    Rust struct equality, never the JSON keys' presence / absence.
//!  - THE CARD HANDOFF SEAM surviving this criterion's purity: [`neighborhood`] is LENS-AGNOSTIC (a
//!    generic BFS over every currently-valid edge, regardless of relation), so a file's `CONTAINS`-
//!    linked entities stay reachable that way - the metadata card's job (criterion 2) - even though
//!    this criterion makes the SAME entities unreachable as cluster-fold NODES. Proving that
//!    boundary needs the real `neighborhood` / served-route seam, which the in-module fold test never
//!    drives.
//!
//! `dash` + `contextgraph` compile on BOTH the default and the `--no-default-features` lane (neither
//! the route nor these DTOs is feature-gated), so this guards the served contract in both lanes.

use std::collections::{BTreeSet, HashMap};

use rigger::contextgraph::{
    Edge, Graph, Node, KIND_CODE_ENTITY, KIND_DECISION, KIND_DESIGN_DOC, KIND_FILE, REL_CONTAINS,
    REL_REFERENCES, TIER_EXTRACTED,
};
use rigger::dash::{
    cluster_detail, clustered_overview, neighborhood, route, Cluster, ClusterEdge, Lens,
};

/// A code-entity node under `<file>::<name>` (so [`file_of`] folds it to `<file>`).
fn ce(id: &str) -> Node {
    Node {
        id: id.to_string(),
        kind: KIND_CODE_ENTITY.to_string(),
        attrs: Default::default(),
    }
}

/// A `KIND_FILE` node - the file's OWN node, distinct from the entities it defines. Purity-excluded
/// from every files-lens cluster (spec 63 c3): it never inflates its own file's count nor renders as
/// a second peer node beside its entities.
fn file_node(id: &str) -> Node {
    Node {
        id: id.to_string(),
        kind: KIND_FILE.to_string(),
        attrs: Default::default(),
    }
}

/// A node of an arbitrary non-code-entity kind (a dev-loop decision, a design-doc): purity-excluded
/// from the files lens entirely - not even its own kind bucket.
fn plain(id: &str, kind: &str) -> Node {
    Node {
        id: id.to_string(),
        kind: kind.to_string(),
        attrs: Default::default(),
    }
}

/// A currently-valid edge (`valid_to = None`) of `rel`.
fn edge(from: &str, to: &str, rel: &str) -> Edge {
    Edge {
        from: from.to_string(),
        to: to.to_string(),
        rel: rel.to_string(),
        valid_from: 0,
        valid_to: None,
        source: 0,
        tier: TIER_EXTRACTED.to_string(),
    }
}

/// A currently-valid REFERENCES edge between two ids.
fn refs(from: &str, to: &str) -> Edge {
    edge(from, to, REL_REFERENCES)
}

/// The lens fixture. TWO files, each with two code entities that call each other internally (adds NO
/// cross-file weight): `src/alpha/a.rs = {foo, bar}`, `src/beta/b.rs = {baz, qux}`. Each file's OWN
/// `KIND_FILE` node sits beside its entities. Plus a membership-less decision and a design-doc, both
/// purity-excluded entirely. TWO edges cross the files (`foo->baz`, `bar->qux`), folding to one
/// symmetric weight-2 super-edge. ONE graph drives every fold, exactly as the browser hits one live
/// graph for the overview and then a drill.
const FOO: &str = "src/alpha/a.rs::foo";
const BAR: &str = "src/alpha/a.rs::bar";
const BAZ: &str = "src/beta/b.rs::baz";
const QUX: &str = "src/beta/b.rs::qux";
const FILE_A: &str = "src/alpha/a.rs";
const FILE_B: &str = "src/beta/b.rs";

fn lens_graph() -> Graph {
    Graph {
        nodes: vec![
            ce(FOO),
            ce(BAR),
            ce(BAZ),
            ce(QUX),
            file_node(FILE_A),
            file_node(FILE_B),
            plain("d1", KIND_DECISION),
            plain("docs/x.md", KIND_DESIGN_DOC),
        ],
        edges: vec![
            // Intra-file coupling (adds NO cross-file weight).
            refs(FOO, BAR),
            refs(BAZ, QUX),
            // Cross-file coupling, twice -> one symmetric weight-2 super-edge.
            refs(FOO, BAZ),
            refs(BAR, QUX),
            // Edges touching the purity-excluded file nodes / decision / design-doc add nothing.
            refs(FOO, FILE_A),
            refs(FOO, "d1"),
            refs(FOO, "docs/x.md"),
        ],
    }
}

/// THE FILES-LENS OVERVIEW over the public crate boundary: `clustered_overview(graph, &Lens::Files)`
/// buckets every code entity by its OWN FILE, sizing each file cluster by CONTAINED-ENTITY count and
/// colouring it by its (necessarily uniform, code-entity) dominant kind; only edges that CROSS two
/// files weight the symmetric super-edge (intra-file coupling and every edge touching a purity-
/// excluded endpoint add none). Spec 63 criterion 3 (FILES-LENS PURITY, the subjects-only rule): the
/// file nodes, the decision, and the design-doc carry NO cluster at all here - not even their own
/// kind bucket - so no storage-schema name ever surfaces as a cluster key.
#[test]
fn files_lens_overview_buckets_code_entities_by_file_and_excludes_every_other_node() {
    let overview = clustered_overview(&lens_graph(), &Lens::Files);

    assert_eq!(
        overview.total, 8,
        "total carries every graph node, the excluded file/decision/design-doc nodes included"
    );
    assert_eq!(
        overview.empty_state, None,
        "the files lens carries no empty state - it is never a derived, underivable grain"
    );
    assert_eq!(
        overview.clusters,
        vec![
            Cluster {
                key: FILE_A.to_string(),
                count: 2,
                kind: KIND_CODE_ENTITY.to_string(),
                label: None,
            },
            Cluster {
                key: FILE_B.to_string(),
                count: 2,
                kind: KIND_CODE_ENTITY.to_string(),
                label: None,
            },
        ],
        "each file becomes one counted Cluster keyed by its own path, sized by its contained-entity \
         count; the file nodes / decision / design-doc carry no cluster at all: {overview:?}"
    );
    assert!(
        overview
            .clusters
            .iter()
            .all(|c| c.label.is_none() && c.key != KIND_DECISION && c.key != KIND_DESIGN_DOC),
        "no storage-schema-name (a kind bucket) and no community-style label ever appears here: {overview:?}"
    );
    assert_eq!(
        overview.edges,
        vec![ClusterEdge {
            from: FILE_A.to_string(),
            to: FILE_B.to_string(),
            weight: 2,
        }],
        "only cross-file coupling weights the super-edge; intra-file edges and edges touching a \
         purity-excluded endpoint add none: {overview:?}"
    );
}

/// Same-file entities MERGE into one cluster; entities in different files - even under the same
/// parent directory - never merge. This is the files lens's OWN sizing rule (a cluster IS a file),
/// distinct from the pre-existing directory fold it supersedes.
#[test]
fn files_lens_merges_same_file_entities_and_keeps_different_files_apart() {
    let graph = Graph {
        nodes: vec![
            ce("lib/a.rs::x"),
            ce("lib/a.rs::y"),
            // A DIFFERENT file, same directory as the pair above - its own cluster, never merged.
            ce("lib/b.rs::z"),
        ],
        edges: vec![],
    };
    let overview = clustered_overview(&graph, &Lens::Files);
    assert_eq!(
        overview.clusters,
        vec![
            Cluster {
                key: "lib/a.rs".to_string(),
                count: 2,
                kind: KIND_CODE_ENTITY.to_string(),
                label: None,
            },
            Cluster {
                key: "lib/b.rs".to_string(),
                count: 1,
                kind: KIND_CODE_ENTITY.to_string(),
                label: None,
            },
        ],
        "same-file entities merge (count 2); a different file never merges, even in the same directory"
    );
}

/// THE FILES-LENS DRILL over the public boundary is UNCONDITIONALLY EMPTY (spec 63 c3): a file is
/// this lens's atomic LEAF subject, so drilling it - a REAL, populated file cluster, an unknown key,
/// or any key over an empty graph - never yields members, never an error.
#[test]
fn files_lens_drill_is_unconditionally_empty_at_the_public_boundary() {
    let graph = lens_graph();

    let real = cluster_detail(&graph, FILE_A, &Lens::Files);
    assert_eq!(
        real.seed, FILE_A,
        "the drill echoes the drilled key as its seed"
    );
    assert_eq!(real.depth, 0, "a cluster drill is not a hop-bounded walk");
    assert!(
        real.nodes.is_empty() && real.edges.is_empty() && real.truncated.is_none(),
        "a REAL, populated file cluster still drills to nothing under files-lens purity: {real:?}"
    );

    let unknown = cluster_detail(&graph, "no/such/file.rs", &Lens::Files);
    assert!(unknown.nodes.is_empty() && unknown.edges.is_empty() && unknown.truncated.is_none());

    let empty_graph = Graph {
        nodes: Vec::new(),
        edges: Vec::new(),
    };
    let none = cluster_detail(&empty_graph, FILE_A, &Lens::Files);
    assert!(none.nodes.is_empty() && none.edges.is_empty() && none.truncated.is_none());
}

/// THE CARD HANDOFF SEAM (criterion 2, not this one's) survives this criterion's cluster-fold purity
/// intact: [`neighborhood`] is LENS-AGNOSTIC - a generic BFS over every currently-valid edge - so a
/// file's `CONTAINS`-linked entities stay reachable there even though the SAME entities are now
/// unreachable as files-lens cluster NODES. This is the exact handoff spec 63's Goal describes: a
/// file's top entities live on the metadata card, not as cluster members.
#[test]
fn a_files_contained_entities_are_still_reachable_via_neighborhood_the_cards_own_seam() {
    let graph = Graph {
        nodes: vec![file_node(FILE_A), ce(FOO), ce(BAR)],
        edges: vec![
            edge(FILE_A, FOO, REL_CONTAINS),
            edge(FILE_A, BAR, REL_CONTAINS),
        ],
    };
    let nb = neighborhood(&graph, FILE_A, 1);
    let ids: BTreeSet<&str> = nb.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(
        ids,
        [FILE_A, FOO, BAR].into_iter().collect(),
        "a depth-1 neighborhood of the file reaches both its CONTAINS-linked entities: {nb:?}"
    );
}

/// Drive the public `route` for `GET <target>` over the lens fixture and return the raw `Response`.
/// `route` is the exact body-builder `serve` ships (serve delegates to it), so this drives the
/// files-lens DEFAULT dispatch the browser hits on every plain `/api/graph` load - the seam the
/// in-process folds never exercise.
fn served(target: &str) -> rigger::dash::Response {
    let graph = lens_graph();
    let liveness: HashMap<String, u64> = HashMap::new();
    let resp = route(
        "GET",
        target,
        &[],
        &graph,
        &[],
        &liveness,
        0,
        "rigger-run",
        "origin/main",
        &[],
    );
    assert_eq!(
        resp.status, 200,
        "GET {target} must be served 200 (the files-lens route never errors on a live graph)"
    );
    resp
}

fn served_json(target: &str) -> serde_json::Value {
    let resp = served(target);
    serde_json::from_slice(&resp.body)
        .unwrap_or_else(|e| panic!("the served {target} body must be valid JSON: {e}"))
}

/// THE SERVED `/api/graph` ROUTE files-lens DEFAULT: a lens-absent request (the exact request every
/// existing spec-30/42 consumer already sends) folds by file, and an absent / explicit `lens=files`
/// carry the byte-identical body - proving this criterion's purity fix reached the actual served
/// default, not just the in-process fold the unit test drives.
#[test]
fn the_served_graph_route_defaults_to_the_purified_files_lens() {
    let default_ov = served_json("/api/graph");
    let keys: BTreeSet<&str> = default_ov["clusters"]
        .as_array()
        .expect("clusters array")
        .iter()
        .map(|c| c["key"].as_str().expect("cluster key is a string"))
        .collect();
    assert_eq!(
        keys,
        [FILE_A, FILE_B].into_iter().collect(),
        "the lens-absent default overview folds by file, excluding the file/decision/design-doc \
         nodes entirely: {default_ov}"
    );
    assert_eq!(
        served("/api/graph?lens=files").body,
        served("/api/graph").body,
        "an explicit lens=files is byte-identical to the lens-absent default"
    );

    // The served drill is unconditionally empty too, over the same live graph.
    let drill = served_json(&format!(
        "/api/graph?cluster={}",
        FILE_A.replace('/', "%2F")
    ));
    assert_eq!(drill["seed"].as_str(), Some(FILE_A));
    assert_eq!(
        drill["nodes"].as_array().expect("drill nodes array").len(),
        0,
        "the served files-lens drill is unconditionally empty: {drill}"
    );
    assert!(
        drill.get("truncated").is_none(),
        "an always-empty drill is always complete, so truncated never serializes: {drill}"
    );
}

/// THE SERIALIZED WIRE-SHAPE back-compat: `Cluster.label` / `ClusterOverview.empty_state` are both
/// `skip_serializing_if = Option::is_none`, so the files overview an external JS panel reads carries
/// NO `label` key on any cluster and NO `empty_state` key at all - byte-identical to before spec 63.
#[test]
fn the_serialized_files_overview_carries_no_label_and_no_empty_state() {
    let overview = serde_json::to_value(clustered_overview(&lens_graph(), &Lens::Files))
        .expect("the files overview serializes to JSON");
    assert!(
        overview.get("empty_state").is_none(),
        "the files overview carries NO empty_state key on the wire: {overview}"
    );
    let clusters = overview["clusters"]
        .as_array()
        .expect("clusters is an array");
    assert!(
        !clusters.is_empty() && clusters.iter().all(|c| c.get("label").is_none()),
        "NO files-lens cluster carries a label key on the wire (byte-identical back-compat): {overview}"
    );
}
