//! Periphery (API / integration / contract) tests for spec 53's CODE LENS VIEW (criterion 4): the
//! pluggable `lens=` selector on the dash's overview / drill folds. `lens=code` buckets a graph node
//! by its DERIVED coupling-community membership at a resolution grain - the SAME whole-graph
//! overview / drill folds, a different bucket key - so the middle-altitude KG view groups the graph
//! by how the code WORKS TOGETHER, not where it sits on disk. `lens=files` (and an absent / unknown
//! lens) is the byte-identical spec-42 directory/kind fold.
//!
//! These run OUTSIDE the crate, over the library's PUBLIC surface (`rigger::dash::{Lens, from_query,
//! clustered_overview, cluster_detail, route, ...}` + the two lens consts), so they guard the exact
//! boundaries the inside-out unit test (`src/dash.rs mod tests`, which reaches the same functions via
//! `super::` and calls the folds in-process) is structurally blind to:
//!
//!  - PUBLIC REACHABILITY. The unit test proves the code-lens BEHAVIOUR but never that `Lens`, its
//!    variants, `Lens::from_query`, the changed-signature `clustered_overview` / `cluster_detail`,
//!    and the `DEFAULT_COMMUNITY_RESOLUTION` / `CODE_LENS_UNDERIVED` consts are all `pub` and
//!    reachable as `rigger::dash::...`. If any were accidentally crate-private, only a crate-external
//!    test fails to COMPILE - the inside-out test would stay green.
//!  - THE SERVED ROUTE END-TO-END. The unit test calls `clustered_overview` / `cluster_detail`
//!    directly and NEVER through `route`'s `lens=` / `resolution=` query parsing (`from_query` wired
//!    to `query_param` + `percent_decode`). A regression that forgot to thread the lens into the
//!    DRILL, or mis-parsed `resolution=`, is invisible in-process; here it reddens, because these
//!    drive the exact body-builder `serve` ships (serve delegates to `route`).
//!  - THE SERIALIZED WIRE-SHAPE back-compat. `Cluster.label` and `ClusterOverview.empty_state` are
//!    both `#[serde(skip_serializing_if = "Option::is_none")]`, so the files overview an external JS
//!    panel reads stays BYTE-IDENTICAL (no `label`, no `empty_state` key), while the code overview
//!    gains a community `label` and an underived grain gains `empty_state`. The unit test asserts
//!    Rust struct equality, never the JSON keys' presence / absence.
//!
//! `dash` + `contextgraph` compile on BOTH the default and the `--no-default-features` lane (neither
//! the route nor these DTOs is feature-gated), so this guards the served contract in both lanes.

use std::collections::{BTreeSet, HashMap};

use rigger::contextgraph::{
    Edge, Graph, Node, KIND_CODE_ENTITY, KIND_COMMUNITY, KIND_DECISION, KIND_DESIGN_DOC, REL_CALLS,
    REL_IN_COMMUNITY, TIER_EXTRACTED, TIER_INFERRED,
};
use rigger::dash::{
    cluster_detail, clustered_overview, route, Cluster, ClusterEdge, Lens, CODE_LENS_UNDERIVED,
    DEFAULT_COMMUNITY_RESOLUTION,
};

// The two default-grain community ids (`community/<resolution>/<n>`) the fixture derives.
const C0: &str = "community/1/0";
const C1: &str = "community/1/1";

/// A code-entity node whose id names a file under a module directory (so the FILES lens folds it by
/// that directory) - the coupling members the CODE lens instead folds by their community.
fn ce(id: &str) -> Node {
    Node {
        id: id.to_string(),
        kind: KIND_CODE_ENTITY.to_string(),
        attrs: Default::default(),
    }
}

/// A derived `KIND_COMMUNITY` super-node carrying its deterministic display `label` attr (the fold's
/// highest-degree-member pick, spec 53 c3). Under the code lens it is a BUCKET, not a member, so it
/// is excluded from every count and never carries its own membership.
fn community(id: &str, label: &str) -> Node {
    let mut n = Node {
        id: id.to_string(),
        kind: KIND_COMMUNITY.to_string(),
        attrs: Default::default(),
    };
    n.attrs.insert("label".to_string(), label.to_string());
    n
}

/// A membership-LESS node of an arbitrary kind (a dev-loop decision, a design doc): under the code
/// lens it must keep its KIND bucket, so the view stays whole-graph.
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

/// The lens fixture. TWO coupling communities, each a pair of code entities in DIFFERENT directories
/// that call each other - so the code lens grouping demonstrably crosses directory lines, the whole
/// point of the lens: `community/1/0 = {foo(src/alpha), bar(src/beta)}`, `community/1/1 =
/// {baz(src/gamma), qux(src/delta)}`. Plus the two derived `KIND_COMMUNITY` super-nodes (each with a
/// deterministic `label`) and TWO membership-less nodes (a decision, a design-doc) that must keep
/// their kind buckets under the code lens. Coupling: each community's pair calls internally (adds no
/// cross-community weight); TWO edges cross the communities (`foo->baz`, `bar->qux`), folding to one
/// symmetric weight-2 super-edge. ONE graph drives every fold, exactly as the browser hits one live
/// graph for the overview and then a drill.
const FOO: &str = "src/alpha/a.rs::foo";
const BAR: &str = "src/beta/b.rs::bar";
const BAZ: &str = "src/gamma/c.rs::baz";
const QUX: &str = "src/delta/d.rs::qux";

fn lens_graph() -> Graph {
    Graph {
        nodes: vec![
            ce(FOO),
            ce(BAR),
            ce(BAZ),
            ce(QUX),
            community(C0, "foo"),
            community(C1, "baz"),
            plain("d1", KIND_DECISION),
            plain("docs/x.md", KIND_DESIGN_DOC),
        ],
        edges: vec![
            // Live memberships at grain 1 (the c3 fold's IN_COMMUNITY spokes).
            edge(FOO, C0, REL_IN_COMMUNITY, TIER_INFERRED),
            edge(BAR, C0, REL_IN_COMMUNITY, TIER_INFERRED),
            edge(BAZ, C1, REL_IN_COMMUNITY, TIER_INFERRED),
            edge(QUX, C1, REL_IN_COMMUNITY, TIER_INFERRED),
            // Intra-community coupling (adds NO cross-community weight).
            edge(FOO, BAR, REL_CALLS, TIER_EXTRACTED),
            edge(BAZ, QUX, REL_CALLS, TIER_EXTRACTED),
            // Cross-community coupling, twice -> one symmetric weight-2 super-edge.
            edge(FOO, BAZ, REL_CALLS, TIER_EXTRACTED),
            edge(BAR, QUX, REL_CALLS, TIER_EXTRACTED),
        ],
    }
}

/// The default code lens (`resolution = DEFAULT_COMMUNITY_RESOLUTION`), reachable as a public type.
fn code_default() -> Lens {
    Lens::Code {
        resolution: DEFAULT_COMMUNITY_RESOLUTION.to_string(),
    }
}

/// THE PUBLIC SELECTOR PARSER (`Lens::from_query`) at the crate boundary. An external caller (the
/// route) relies on it being TOTAL and INFALLIBLE: `lens=code` selects the code fold at `resolution=`
/// (defaulting to `DEFAULT_COMMUNITY_RESOLUTION` when absent OR empty), and every other value - an
/// explicit `files`, an unknown lens, or an absent one - falls back to the byte-identical
/// `Lens::Files`, so a hostile selector can never error the route. The inside-out test asserts the
/// same table via `super::`; this pins that `Lens`, its variants, `from_query`, and the default-grain
/// const are all reachable as `rigger::dash::...` - a boundary a same-crate test cannot prove.
#[test]
fn lens_from_query_is_a_public_total_selector_that_falls_back_to_files() {
    assert_eq!(
        Lens::from_query(Some("code"), None),
        code_default(),
        "lens=code with no resolution selects the code fold at the DEFAULT grain"
    );
    assert_eq!(
        Lens::from_query(Some("code"), Some("")),
        code_default(),
        "an EMPTY resolution still defaults to the default grain (never an empty-string grain)"
    );
    assert_eq!(
        Lens::from_query(Some("code"), Some("1.5")),
        Lens::Code {
            resolution: "1.5".to_string()
        },
        "an explicit resolution grain is honoured verbatim"
    );
    assert_eq!(
        Lens::from_query(None, None),
        Lens::Files,
        "an ABSENT lens is the byte-identical Files default"
    );
    assert_eq!(
        Lens::from_query(Some("files"), None),
        Lens::Files,
        "an explicit lens=files is the Files default"
    );
    assert_eq!(
        Lens::from_query(Some("bogus"), Some("9")),
        Lens::Files,
        "an UNKNOWN lens falls back to Files (a hostile selector never errors), ignoring resolution"
    );
}

/// THE CODE-LENS OVERVIEW over the public crate boundary: `clustered_overview(graph, &Lens::Code)`
/// buckets every membership-carrying node by its coupling COMMUNITY - a subsystem grouped ACROSS
/// directory lines - sizing each community super-node by MEMBER count, colouring it by its dominant
/// member kind, and labelling it with the community node's deterministic `label`; a membership-LESS
/// node keeps its KIND bucket (so the view stays whole-graph); and only edges that CROSS two
/// communities weight the symmetric super-edge (intra-community coupling and the membership spokes to
/// the excluded super-node add none). Every value is bound to the fixture so a renamed field or a
/// mis-fold reddens here, not just in-process.
#[test]
fn code_lens_overview_buckets_members_by_community_across_directories() {
    let overview = clustered_overview(&lens_graph(), &code_default());

    assert_eq!(
        overview.total, 8,
        "total carries every graph node, the excluded community super-nodes included"
    );
    assert_eq!(
        overview.empty_state, None,
        "a DERIVED grain is not the empty state"
    );
    assert_eq!(
        overview.clusters,
        vec![
            // Each community: sized by MEMBER count (2, the excluded super-node never inflates it),
            // coloured by dominant member kind, labelled by the community node's deterministic label.
            Cluster {
                key: C0.to_string(),
                count: 2,
                kind: KIND_CODE_ENTITY.to_string(),
                label: Some("foo".to_string()),
            },
            Cluster {
                key: C1.to_string(),
                count: 2,
                kind: KIND_CODE_ENTITY.to_string(),
                label: Some("baz".to_string()),
            },
            // Membership-less nodes keep their KIND buckets (NOT their directory buckets), so the
            // code lens stays whole-graph.
            Cluster {
                key: KIND_DECISION.to_string(),
                count: 1,
                kind: KIND_DECISION.to_string(),
                label: None,
            },
            Cluster {
                key: KIND_DESIGN_DOC.to_string(),
                count: 1,
                kind: KIND_DESIGN_DOC.to_string(),
                label: None,
            },
        ],
        "code lens folds members by community (sized, dominant-kind, labelled) and keeps kind buckets for membership-less nodes"
    );
    assert_eq!(
        overview.edges,
        vec![ClusterEdge {
            from: C0.to_string(),
            to: C1.to_string(),
            weight: 2,
        }],
        "only cross-community coupling weights the super-edge; intra-community edges and membership spokes to the excluded super-node add none"
    );
}

/// THE CODE-LENS DRILL over the public boundary: `cluster_detail(graph, community_key, &Lens::Code)`
/// yields EXACTLY that community's member nodes and the coupling edges AMONG them. The excluded
/// super-node is not a member; a membership spoke to it is not an intra-community edge; and a
/// cross-community edge to a non-member is not among them - so none of the three render.
#[test]
fn code_lens_drill_yields_exactly_the_community_members_and_their_intra_edges() {
    let drill = cluster_detail(&lens_graph(), C0, &code_default());

    assert_eq!(drill.seed, C0, "the drill echoes the drilled community key");
    assert_eq!(drill.truncated, None, "a small community renders whole");

    let members: BTreeSet<&str> = drill.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(
        members,
        [FOO, BAR].into_iter().collect::<BTreeSet<&str>>(),
        "the community drills to exactly its member code entities: {drill:?}"
    );
    assert_eq!(
        drill.edges.len(),
        1,
        "only the intra-community coupling edge (foo->bar) renders; the membership spoke and the cross-community edge do not: {drill:?}"
    );
    assert!(
        drill.edges.iter().all(|e| e.rel == REL_CALLS
            && members.contains(e.from.as_str())
            && members.contains(e.to.as_str())),
        "every drill edge is intra-community coupling between two members: {drill:?}"
    );
}

/// THE UNDERIVED-GRAIN empty state over the public boundary: a code lens at a resolution grain with
/// NO derived assignments returns the documented `CODE_LENS_UNDERIVED` prompt - never an error and
/// never a bare kind-bucket view - while `total` still reports the whole graph size.
#[test]
fn code_lens_at_an_underived_grain_carries_the_documented_empty_state_not_an_error() {
    let underived = clustered_overview(
        &lens_graph(),
        &Lens::Code {
            resolution: "2".to_string(),
        },
    );

    assert!(
        underived.clusters.is_empty() && underived.edges.is_empty(),
        "an underived grain folds no communities: {underived:?}"
    );
    assert_eq!(
        underived.total, 8,
        "the empty state still reports the whole graph size"
    );
    assert_eq!(
        underived.empty_state.as_deref(),
        Some(CODE_LENS_UNDERIVED),
        "an underived grain carries the documented empty-state message, never an error"
    );
}

/// THE SERIALIZED WIRE-SHAPE back-compat the external panel reads: this pins the JSON keys'
/// presence / absence, which the struct-equality inside-out test cannot. Both `Cluster.label` and
/// `ClusterOverview.empty_state` are `skip_serializing_if = Option::is_none`, so:
///   * the FILES overview JSON is byte-identical to before spec 53 - NO cluster carries a `label`
///     key and the body carries NO `empty_state` key;
///   * a CODE overview's community cluster DOES carry `label`, and an UNDERIVED code overview DOES
///     carry `empty_state`.
#[test]
fn the_serialized_overview_skips_label_and_empty_state_off_the_files_lens() {
    let graph = lens_graph();

    // --- FILES lens: byte-identical wire shape (no label anywhere, no empty_state) ---
    let files = serde_json::to_value(clustered_overview(&graph, &Lens::Files))
        .expect("the files overview serializes to JSON");
    assert!(
        files.get("empty_state").is_none(),
        "the files overview carries NO empty_state key on the wire: {files}"
    );
    let files_clusters = files["clusters"]
        .as_array()
        .expect("clusters is a JSON array");
    assert!(
        files_clusters.iter().all(|c| c.get("label").is_none()),
        "NO files-lens cluster carries a label key on the wire (byte-identical back-compat): {files}"
    );

    // --- CODE lens: a community cluster carries its label; a derived overview has no empty_state ---
    let code = serde_json::to_value(clustered_overview(&graph, &code_default()))
        .expect("the code overview serializes to JSON");
    assert!(
        code.get("empty_state").is_none(),
        "a DERIVED code overview carries no empty_state key: {code}"
    );
    let community_label = code["clusters"]
        .as_array()
        .expect("clusters is a JSON array")
        .iter()
        .find(|c| c["key"] == C0)
        .and_then(|c| c.get("label"))
        .and_then(|l| l.as_str());
    assert_eq!(
        community_label,
        Some("foo"),
        "the code overview's community cluster carries its deterministic label on the wire: {code}"
    );

    // --- UNDERIVED code lens: the empty_state key IS present and carries the documented message ---
    let underived = serde_json::to_value(clustered_overview(
        &graph,
        &Lens::Code {
            resolution: "2".to_string(),
        },
    ))
    .expect("the underived overview serializes to JSON");
    assert_eq!(
        underived.get("empty_state").and_then(|v| v.as_str()),
        Some(CODE_LENS_UNDERIVED),
        "an underived code overview carries the empty_state message key on the wire: {underived}"
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

/// THE SERVED `/api/graph` ROUTE threads the `lens=` / `resolution=` selector END-TO-END into BOTH
/// the overview and the drill - the integration seam the in-process folds never cover:
///   * `?lens=code` folds the overview by community, carrying the community `label`;
///   * `?lens=code&cluster=<id>` drills that community to exactly its members (the lens reaches the
///     DRILL branch too, not just the overview);
///   * `?lens=code&resolution=` (empty) defaults to the same derived grain as an explicit `1`;
///   * `?lens=code&resolution=2` (underived) carries the `empty_state` prompt, never an error;
///   * an absent lens, `?lens=files`, and a hostile `?lens=bogus` are ALL byte-identical (the
///     spec-42 default the browser and every spec-30/42 request already receive).
#[test]
fn the_served_graph_route_threads_the_lens_selector_into_overview_and_drill() {
    // --- CODE overview via the route: community-bucketed, labelled ---
    let code_ov = served_json("/api/graph?lens=code&resolution=1");
    let keys: Vec<&str> = code_ov["clusters"]
        .as_array()
        .expect("clusters array")
        .iter()
        .map(|c| c["key"].as_str().expect("cluster key is a string"))
        .collect();
    assert_eq!(
        keys,
        vec![C0, C1, KIND_DECISION, KIND_DESIGN_DOC],
        "the served code overview buckets by community + kind: {code_ov}"
    );
    assert_eq!(
        code_ov["clusters"][0]["label"].as_str(),
        Some("foo"),
        "the served community cluster carries its label: {code_ov}"
    );

    // An EMPTY resolution defaults to grain 1: the body is identical to the explicit-grain request.
    assert_eq!(
        served("/api/graph?lens=code&resolution=").body,
        served("/api/graph?lens=code&resolution=1").body,
        "an empty resolution= defaults to the same derived grain as resolution=1"
    );

    // --- CODE drill via the route: the lens reaches the cluster= branch, drilling a community ---
    let drill = served_json("/api/graph?lens=code&cluster=community/1/0");
    assert_eq!(
        drill["seed"].as_str(),
        Some(C0),
        "the served drill echoes the community key: {drill}"
    );
    let members: BTreeSet<&str> = drill["nodes"]
        .as_array()
        .expect("drill nodes array")
        .iter()
        .map(|n| n["id"].as_str().expect("node id is a string"))
        .collect();
    assert_eq!(
        members,
        [FOO, BAR].into_iter().collect::<BTreeSet<&str>>(),
        "the served community drill yields exactly its members: {drill}"
    );

    // --- UNDERIVED grain via the route: the empty_state prompt, never a 500 ---
    let underived = served_json("/api/graph?lens=code&resolution=2");
    assert_eq!(
        underived["empty_state"].as_str(),
        Some(CODE_LENS_UNDERIVED),
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
    // The files default is genuinely NOT the code view (proves the comparison above is meaningful).
    assert_ne!(
        default,
        served("/api/graph?lens=code&resolution=1").body,
        "the code lens actually changes the served body (the back-compat equality is not vacuous)"
    );
}
