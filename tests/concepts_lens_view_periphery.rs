//! Periphery (API / integration / contract) tests for spec 54's CONCEPTS LENS VIEW (criterion 3):
//! the `lens=concepts` selector on the dash's overview / drill folds. `lens=concepts` buckets a graph
//! node by the intent CONCEPT it REALIZES at a resolution grain - the idea the docs and code realize,
//! grouped ACROSS directory lines - through the SAME whole-graph overview / drill folds the files and
//! code lenses use, a different bucket key. A node realizing MORE THAN ONE concept folds under its
//! PRIMARY (the largest concept by member count, ties by lexicographically-smallest id) and is
//! flagged `shared` so it is counted once, never silently duplicated; a membership-less node keeps
//! its KIND bucket (so the view stays whole-graph); the `KIND_CONCEPT` super-node is a bucket, not a
//! member, so it is excluded. `lens=files` (and an absent / unknown lens) stays the byte-identical
//! spec-42 directory/kind fold.
//!
//! These run OUTSIDE the crate, over the library's PUBLIC surface (`rigger::dash::{Lens, from_query,
//! clustered_overview, cluster_detail, route, NeighborhoodNode.shared, ...}` + the two concept
//! consts), so they guard the exact boundaries the inside-out unit test (`src/dash.rs mod tests`,
//! which reaches the same functions via `super::` and calls the folds in-process) is structurally
//! blind to:
//!
//!  - PUBLIC REACHABILITY. The unit test proves the concepts-lens BEHAVIOUR but never that
//!    `Lens::Concepts`, `Lens::from_query`, the concepts arms of `clustered_overview` /
//!    `cluster_detail`, the new `NeighborhoodNode.shared` field, and the `DEFAULT_CONCEPT_RESOLUTION`
//!    / `CONCEPTS_LENS_UNDERIVED` consts are all `pub` and reachable as `rigger::dash::...`. If any
//!    were accidentally crate-private, only a crate-external test fails to COMPILE - the inside-out
//!    test would stay green.
//!  - THE SERVED ROUTE END-TO-END. The unit test calls `clustered_overview` / `cluster_detail`
//!    directly and NEVER through `route`'s `lens=` / `resolution=` query parsing (`from_query` wired
//!    to `query_param` + `percent_decode`). A regression that forgot to thread the concepts lens into
//!    the DRILL, or mis-parsed `resolution=`, is invisible in-process; here it reddens, because these
//!    drive the exact body-builder `serve` ships (serve delegates to `route`).
//!  - THE SERIALIZED `shared` WIRE-SHAPE back-compat. `NeighborhoodNode.shared` is
//!    `#[serde(skip_serializing_if = "is_not_shared", default)]`, so a single-concept / membership-less
//!    drill node and EVERY non-concepts view serialize BYTE-IDENTICALLY to before this lens existed
//!    (no `shared` key), while a genuinely-shared concept member gains `"shared": true`. The unit test
//!    asserts the Rust `bool`, never the JSON key's presence / absence.
//!
//! `dash` + `contextgraph` compile on BOTH the default and the `--no-default-features` lane (neither
//! the route nor these DTOs is feature-gated), so this guards the served contract in both lanes.

use std::collections::{BTreeSet, HashMap};

use rigger::contextgraph::{
    Edge, Graph, Node, KIND_CODE_ENTITY, KIND_CONCEPT, KIND_DECISION, KIND_DESIGN_DOC, REL_CALLS,
    REL_REALIZES, REL_REFERENCES, TIER_INFERRED,
};
use rigger::dash::{
    cluster_detail, clustered_overview, route, Cluster, ClusterEdge, Lens, CONCEPTS_LENS_UNDERIVED,
    DEFAULT_CONCEPT_RESOLUTION,
};

// The two default-grain concept ids (`concept/<resolution>/<n>`) the fixture derives.
const C0: &str = "concept/1/0";
const C1: &str = "concept/1/1";

// The fixture's node ids. Every concept member sits in a DIFFERENT directory, so a concept grouping
// demonstrably crosses directory lines - the whole point of the lens (a files fold would scatter these
// across `docs`, `src/store`, `src/index`).
const STORE_DOC: &str = "docs/store.md";
const API_DOC: &str = "docs/api.md";
const APPEND: &str = "src/store/log.rs::append";
const INDEX: &str = "src/index/build.rs::index";
const HELPER: &str = "src/util/misc.rs::helper";

/// A code-entity node whose id names a file under a module directory (so the FILES lens folds it by
/// that directory) - the members the CONCEPTS lens instead folds by the concept they realize.
fn ce(id: &str) -> Node {
    Node {
        id: id.to_string(),
        kind: KIND_CODE_ENTITY.to_string(),
        attrs: Default::default(),
    }
}

/// A design-doc node: under the concepts lens it folds by the concept it realizes alongside the code,
/// grouping the idea's prose with its implementation across directory lines.
fn doc(id: &str) -> Node {
    Node {
        id: id.to_string(),
        kind: KIND_DESIGN_DOC.to_string(),
        attrs: Default::default(),
    }
}

/// A derived `KIND_CONCEPT` super-node carrying its deterministic display `label` attr (the intent
/// derivation's pick, spec 54). Under the concepts lens it is a BUCKET, not a member, so it is
/// excluded from every count and never carries its own membership.
fn concept(id: &str, label: &str) -> Node {
    let mut n = Node {
        id: id.to_string(),
        kind: KIND_CONCEPT.to_string(),
        attrs: Default::default(),
    };
    n.attrs.insert("label".to_string(), label.to_string());
    n
}

/// A membership-LESS node of an arbitrary kind (a dev-loop decision): under the concepts lens it must
/// keep its KIND bucket, so the view stays whole-graph.
fn plain(id: &str, kind: &str) -> Node {
    Node {
        id: id.to_string(),
        kind: kind.to_string(),
        attrs: Default::default(),
    }
}

/// A currently-valid edge (`valid_to = None`) of `rel` at `tier`.
fn edge(from: &str, to: &str, rel: &str, tier: &str) -> Edge {
    Edge {
        from: from.to_string(),
        to: to.to_string(),
        rel: rel.to_string(),
        valid_from: 0,
        valid_to: None,
        source: 0,
        tier: tier.to_string(),
    }
}

/// The lens fixture. TWO derived concepts, each grouping a DOC with the code it governs across
/// directory lines: `concept/1/0` "the store" = {docs/store.md, src/store/log.rs::append,
/// src/index/build.rs::index} (size 3, the LARGER); `concept/1/1` "the api" = {docs/api.md,
/// src/store/log.rs::append} (size 2, the SMALLER). `append` REALIZES BOTH - a SHARED member whose
/// PRIMARY is the larger `concept/1/0` (by size), so it folds there and is flagged `shared`, counted
/// once. Plus the two `KIND_CONCEPT` super-nodes (each labelled) and TWO membership-less nodes (an
/// unattached code entity + a decision) that keep their KIND buckets. Coupling: ONE intra-concept
/// `append->index` call (adds no cross weight, renders in the c0 drill); ONE cross-concept
/// `store.md->api.md` reference (folds to a single weight-1 super-edge). ONE graph drives every fold,
/// exactly as the browser hits one live graph for the overview and then a drill.
fn lens_graph() -> Graph {
    Graph {
        nodes: vec![
            doc(STORE_DOC),
            doc(API_DOC),
            ce(APPEND),
            ce(INDEX),
            ce(HELPER),
            concept(C0, "the store"),
            concept(C1, "the api"),
            plain("d1", KIND_DECISION),
        ],
        edges: vec![
            // Live REALIZES memberships at grain 1 (member --REALIZES--> concept).
            edge(STORE_DOC, C0, REL_REALIZES, TIER_INFERRED),
            edge(APPEND, C0, REL_REALIZES, TIER_INFERRED),
            edge(INDEX, C0, REL_REALIZES, TIER_INFERRED),
            edge(API_DOC, C1, REL_REALIZES, TIER_INFERRED),
            // The SHARED member: append also realizes the smaller concept/1/1.
            edge(APPEND, C1, REL_REALIZES, TIER_INFERRED),
            // ONE intra-concept coupling edge (append and index both in c0) -> renders in the c0 drill,
            // adds NO cross weight.
            edge(APPEND, INDEX, REL_CALLS, TIER_INFERRED),
            // ONE cross-concept reference (store.md in c0, api.md in c1) -> one weight-1 super-edge.
            edge(STORE_DOC, API_DOC, REL_REFERENCES, TIER_INFERRED),
        ],
    }
}

/// The default concepts lens (`resolution = DEFAULT_CONCEPT_RESOLUTION`), reachable as a public type.
fn concepts_default() -> Lens {
    Lens::Concepts {
        resolution: DEFAULT_CONCEPT_RESOLUTION.to_string(),
    }
}

/// THE PUBLIC SELECTOR PARSER (`Lens::from_query`) at the crate boundary, for the concepts arm. The
/// route relies on it being TOTAL and INFALLIBLE: `lens=concepts` selects the concepts fold at
/// `resolution=` (defaulting to `DEFAULT_CONCEPT_RESOLUTION` when absent OR empty), an explicit grain
/// is honoured verbatim, and every other value - an unknown lens or an absent one - still falls back
/// to the byte-identical `Lens::Files`, so a hostile selector can never error the route. The
/// inside-out test asserts the same table via `super::`; this pins that `Lens::Concepts`, `from_query`,
/// and the default-grain const are reachable as `rigger::dash::...` - a boundary a same-crate test
/// cannot prove.
#[test]
fn lens_from_query_is_a_public_total_selector_including_concepts() {
    assert_eq!(
        Lens::from_query(Some("concepts"), None),
        concepts_default(),
        "lens=concepts with no resolution selects the concepts fold at the DEFAULT grain"
    );
    assert_eq!(
        Lens::from_query(Some("concepts"), Some("")),
        concepts_default(),
        "an EMPTY resolution still defaults to the default grain (never an empty-string grain)"
    );
    assert_eq!(
        Lens::from_query(Some("concepts"), Some("1.5")),
        Lens::Concepts {
            resolution: "1.5".to_string()
        },
        "an explicit resolution grain is honoured verbatim"
    );
    assert_eq!(
        Lens::from_query(Some("bogus"), Some("9")),
        Lens::Files,
        "an UNKNOWN lens still falls back to Files (a hostile selector never errors), ignoring resolution"
    );
    assert_eq!(
        Lens::from_query(None, None),
        Lens::Files,
        "an ABSENT lens is the byte-identical Files default"
    );
}

/// THE CONCEPTS-LENS OVERVIEW over the public crate boundary: `clustered_overview(graph,
/// &Lens::Concepts)` buckets every REALIZES-carrying node by the concept it realizes - an idea grouped
/// ACROSS directory lines - sizing each concept super-node by MEMBER count, colouring it by its
/// dominant member kind, and labelling it with the concept node's deterministic `label`. A node
/// realizing MORE THAN ONE concept folds under its PRIMARY (the larger by member count) and is counted
/// ONCE there; a membership-LESS node keeps its KIND bucket (so the view stays whole-graph); and only
/// edges that CROSS two concepts weight the symmetric super-edge (intra-concept coupling and the
/// REALIZES spokes to the excluded super-node add none). Every value is bound to the fixture so a
/// renamed field or a mis-fold reddens here, not just in-process.
#[test]
fn concepts_lens_overview_buckets_members_by_concept_across_directories() {
    let overview = clustered_overview(&lens_graph(), &concepts_default());

    assert_eq!(
        overview.total, 8,
        "total carries every graph node, the excluded concept super-nodes included"
    );
    assert_eq!(
        overview.empty_state, None,
        "a DERIVED grain is not the empty state"
    );
    assert_eq!(
        overview.clusters,
        vec![
            // The unattached code entity keeps its KIND bucket (NOT its src/util directory), so the
            // concepts lens stays whole-graph.
            Cluster {
                key: KIND_CODE_ENTITY.to_string(),
                count: 1,
                kind: KIND_CODE_ENTITY.to_string(),
                label: None,
            },
            // concept/1/0 (the larger): {store.md, append, index} = 3 members across three
            // directories, dominant kind code-entity (append + index), labelled by the concept node.
            Cluster {
                key: C0.to_string(),
                count: 3,
                kind: KIND_CODE_ENTITY.to_string(),
                label: Some("the store".to_string()),
            },
            // concept/1/1 (the smaller): the SHARED append folds under its primary c0, so c1 counts
            // ONLY its sole non-shared member docs/api.md - never silently duplicated.
            Cluster {
                key: C1.to_string(),
                count: 1,
                kind: KIND_DESIGN_DOC.to_string(),
                label: Some("the api".to_string()),
            },
            // The membership-less decision keeps its KIND bucket.
            Cluster {
                key: KIND_DECISION.to_string(),
                count: 1,
                kind: KIND_DECISION.to_string(),
                label: None,
            },
        ],
        "concepts lens folds members by concept (primary bucket, shared counted once), keeps kind buckets for the unattached nodes, and labels each concept: {overview:?}"
    );
    assert_eq!(
        overview.edges,
        vec![ClusterEdge {
            from: C0.to_string(),
            to: C1.to_string(),
            weight: 1,
        }],
        "only the cross-concept reference weights the super-edge; the intra-concept call and the REALIZES spokes to the excluded super-node add none: {overview:?}"
    );
}

/// THE CONCEPTS-LENS DRILL over the public boundary: `cluster_detail(graph, concept_key,
/// &Lens::Concepts)` yields EXACTLY that concept's member nodes and the coupling edges AMONG them, and
/// FLAGS the member that realizes more than one concept with `shared = true`. The SHARED member
/// appears ONCE - under its PRIMARY concept, never in the smaller concept it also realizes - so a
/// multi-concept node is never silently duplicated across drills.
#[test]
fn concepts_lens_drill_flags_the_shared_member_and_folds_it_under_its_primary() {
    let graph = lens_graph();

    // --- DRILL c0 (the primary of the shared member): its three members, append flagged shared ---
    let drill0 = cluster_detail(&graph, C0, &concepts_default());
    assert_eq!(drill0.seed, C0, "the drill echoes the drilled concept key");
    assert_eq!(drill0.truncated, None, "a small concept renders whole");

    let members0: std::collections::BTreeMap<&str, bool> = drill0
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.shared))
        .collect();
    assert_eq!(
        members0,
        std::collections::BTreeMap::from([(STORE_DOC, false), (APPEND, true), (INDEX, false)]),
        "concept/1/0 drills to exactly {{store.md, append, index}}; the multi-concept append carries shared=true, the single-concept members shared=false: {drill0:?}"
    );
    assert_eq!(
        drill0.edges.len(),
        1,
        "only the intra-concept append->index coupling renders; the REALIZES spokes and the cross-concept reference do not: {drill0:?}"
    );

    // --- DRILL c1 (the smaller concept append also realizes): the shared append is NOT here ---
    let drill1 = cluster_detail(&graph, C1, &concepts_default());
    let members1: BTreeSet<&str> = drill1.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(
        members1,
        [API_DOC].into_iter().collect::<BTreeSet<&str>>(),
        "concept/1/1 drills to ONLY its non-shared member docs/api.md; the shared append appears once, under its primary c0: {drill1:?}"
    );
    assert!(
        drill1.nodes.iter().all(|n| !n.shared),
        "docs/api.md realizes only concept/1/1, so it is not shared: {drill1:?}"
    );
}

/// THE UNDERIVED-GRAIN empty state over the public boundary: a concepts lens at a resolution grain
/// with NO derived assignments returns the documented `CONCEPTS_LENS_UNDERIVED` prompt - never an
/// error and never a bare kind-bucket view - while `total` still reports the whole graph size.
#[test]
fn concepts_lens_at_an_underived_grain_carries_the_documented_empty_state_not_an_error() {
    let underived = clustered_overview(
        &lens_graph(),
        &Lens::Concepts {
            resolution: "2".to_string(),
        },
    );

    assert!(
        underived.clusters.is_empty() && underived.edges.is_empty(),
        "an underived concepts grain folds no concepts: {underived:?}"
    );
    assert_eq!(
        underived.total, 8,
        "the empty state still reports the whole graph size"
    );
    assert_eq!(
        underived.empty_state.as_deref(),
        Some(CONCEPTS_LENS_UNDERIVED),
        "an underived concepts grain carries the documented empty-state message, never an error"
    );
}

/// THE SERIALIZED `shared` WIRE-SHAPE back-compat the external panel reads: this pins the JSON key's
/// presence / absence, which the struct-`bool` inside-out test cannot. `NeighborhoodNode.shared` is
/// `skip_serializing_if = is_not_shared`, so:
///   * a CONCEPTS drill's genuinely-shared member DOES carry `"shared": true`;
///   * every SINGLE-concept concepts-drill node OMITS the key (byte-identical to before this lens);
///   * a FILES-lens drill of the SAME shared-under-concepts node OMITS the key too - the marker is
///     concepts-lens-only, so every other view stays byte-identical on the wire.
#[test]
fn the_serialized_drill_skips_the_shared_marker_off_every_non_shared_node() {
    let graph = lens_graph();

    // --- CONCEPTS drill c0: the shared append carries shared=true; the single-concept members omit it ---
    let c0 = serde_json::to_value(cluster_detail(&graph, C0, &concepts_default()))
        .expect("the concepts drill serializes to JSON");
    let nodes0 = c0["nodes"].as_array().expect("drill nodes is a JSON array");
    let shared_node = nodes0
        .iter()
        .find(|n| n["id"] == APPEND)
        .expect("the drill includes the shared member append");
    assert_eq!(
        shared_node.get("shared").and_then(|v| v.as_bool()),
        Some(true),
        "the shared concept member carries \"shared\": true on the wire: {c0}"
    );
    for id in [STORE_DOC, INDEX] {
        let single = nodes0
            .iter()
            .find(|n| n["id"] == id)
            .unwrap_or_else(|| panic!("the drill includes {id}"));
        assert!(
            single.get("shared").is_none(),
            "the single-concept member {id} carries NO shared key on the wire (byte-identical back-compat): {c0}"
        );
    }

    // --- FILES drill of append's directory: the SAME node omits the shared key (concepts-lens-only) ---
    let files = serde_json::to_value(cluster_detail(&graph, "src/store", &Lens::Files))
        .expect("the files drill serializes to JSON");
    let files_append = files["nodes"]
        .as_array()
        .expect("files drill nodes is a JSON array")
        .iter()
        .find(|n| n["id"] == APPEND)
        .expect("the files drill of src/store includes append");
    assert!(
        files_append.get("shared").is_none(),
        "under the files lens the shared-under-concepts append carries NO shared key - every non-concepts view is byte-identical: {files}"
    );
}

/// Drive the public `route` for `GET <target>` over the lens fixture and return the raw `Response`.
/// `route` is the exact body-builder `serve` ships (serve delegates to it), so this drives the lens
/// selector through the SAME `query_param` + `percent_decode` + `Lens::from_query` wiring the browser
/// hits - the seam the in-process folds never exercise.
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
        "GET {target} must be served 200 (the lens route never errors on a live graph)"
    );
    resp
}

/// Parse a served body as JSON.
fn served_json(target: &str) -> serde_json::Value {
    let resp = served(target);
    serde_json::from_slice(&resp.body)
        .unwrap_or_else(|e| panic!("the served {target} body must be valid JSON: {e}"))
}

/// THE SERVED `/api/graph` ROUTE threads the `lens=concepts` / `resolution=` selector END-TO-END into
/// BOTH the overview and the drill - the integration seam the in-process folds never cover:
///   * `?lens=concepts` folds the overview by concept, carrying the concept `label`;
///   * `?lens=concepts&resolution=` (empty) defaults to the same derived grain as an explicit `1`;
///   * `?lens=concepts&cluster=<id>` drills that concept to exactly its members (the lens reaches the
///     DRILL branch too, not just the overview) AND flags the shared member `"shared": true` on the
///     served wire;
///   * `?lens=concepts&resolution=2` (underived) carries the `empty_state` prompt, never an error;
///   * an absent lens, `?lens=files`, and a hostile `?lens=bogus` are ALL byte-identical (the spec-42
///     default the browser and every spec-30/42 request already receive), and genuinely NOT the
///     concepts body.
#[test]
fn the_served_graph_route_threads_the_concepts_lens_into_overview_and_drill() {
    // --- CONCEPTS overview via the route: concept-bucketed, labelled ---
    let ov = served_json("/api/graph?lens=concepts&resolution=1");
    let keys: Vec<&str> = ov["clusters"]
        .as_array()
        .expect("clusters array")
        .iter()
        .map(|c| c["key"].as_str().expect("cluster key is a string"))
        .collect();
    assert_eq!(
        keys,
        vec![KIND_CODE_ENTITY, C0, C1, KIND_DECISION],
        "the served concepts overview buckets by concept + kind: {ov}"
    );
    let c0_label = ov["clusters"]
        .as_array()
        .expect("clusters array")
        .iter()
        .find(|c| c["key"] == C0)
        .and_then(|c| c.get("label"))
        .and_then(|l| l.as_str());
    assert_eq!(
        c0_label,
        Some("the store"),
        "the served concept cluster carries its label: {ov}"
    );

    // An EMPTY resolution defaults to grain 1: the body is identical to the explicit-grain request.
    assert_eq!(
        served("/api/graph?lens=concepts&resolution=").body,
        served("/api/graph?lens=concepts&resolution=1").body,
        "an empty resolution= defaults to the same derived grain as resolution=1"
    );

    // --- CONCEPTS drill via the route: the lens reaches the cluster= branch AND the shared marker
    // rides the served wire ---
    let drill = served_json("/api/graph?lens=concepts&cluster=concept/1/0");
    assert_eq!(
        drill["seed"].as_str(),
        Some(C0),
        "the served drill echoes the concept key: {drill}"
    );
    let members: BTreeSet<&str> = drill["nodes"]
        .as_array()
        .expect("drill nodes array")
        .iter()
        .map(|n| n["id"].as_str().expect("node id is a string"))
        .collect();
    assert_eq!(
        members,
        [STORE_DOC, APPEND, INDEX]
            .into_iter()
            .collect::<BTreeSet<&str>>(),
        "the served concept drill yields exactly its primary members: {drill}"
    );
    let served_shared = drill["nodes"]
        .as_array()
        .expect("drill nodes array")
        .iter()
        .find(|n| n["id"] == APPEND)
        .and_then(|n| n.get("shared"))
        .and_then(|v| v.as_bool());
    assert_eq!(
        served_shared,
        Some(true),
        "the served drill flags the shared member append with \"shared\": true: {drill}"
    );

    // --- UNDERIVED grain via the route: the empty_state prompt, never a 500 ---
    let underived = served_json("/api/graph?lens=concepts&resolution=2");
    assert_eq!(
        underived["empty_state"].as_str(),
        Some(CONCEPTS_LENS_UNDERIVED),
        "the served underived grain carries the empty-state prompt: {underived}"
    );

    // --- BACK-COMPAT: absent / files / hostile lens are all the byte-identical spec-42 default ---
    let default = served("/api/graph").body;
    assert_eq!(
        served("/api/graph?lens=files").body,
        default,
        "an explicit lens=files is byte-identical to the lens-absent default"
    );
    assert_eq!(
        served("/api/graph?lens=bogus").body,
        default,
        "a hostile lens=bogus falls back byte-identical to the default (never a 500)"
    );
    // The files default is genuinely NOT the concepts view (proves the comparison above is meaningful).
    assert_ne!(
        default,
        served("/api/graph?lens=concepts&resolution=1").body,
        "the concepts lens actually changes the served body (the back-compat equality is not vacuous)"
    );
}
