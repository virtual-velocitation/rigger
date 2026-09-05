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

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rigger::contextgraph::{
    Edge, Graph, Node, KIND_CODE_ENTITY, KIND_DECISION, KIND_DESIGN_DOC, KIND_FILE, REL_CONTAINS,
    REL_REFERENCES, TIER_EXTRACTED,
};
use rigger::dash::{
    cluster_detail, clustered_overview, neighborhood, route, Cluster, ClusterEdge, Lens,
    WHOLE_GRAPH_FILES_UNRESOLVED,
};

/// A code-entity DEFINITION node under `<file>::<name>` (so [`file_of`] folds it to `<file>`),
/// carrying a `name` attr matching its own entity-name suffix - exactly as the extraction fold always
/// sets for a REAL definition, the files-lens honesty gate's (spec 63 c3) marker that this is not a
/// bare cross-file placeholder. Every fixture in this file models an entity's own file, never a
/// cross-file reference, so this is the correct shape throughout; [`bare_ce`] below is the dedicated
/// placeholder shape the honesty-gate tests need.
fn ce(id: &str) -> Node {
    let name = id.rsplit_once("::").map_or(id, |(_, n)| n);
    Node {
        id: id.to_string(),
        kind: KIND_CODE_ENTITY.to_string(),
        attrs: BTreeMap::from([("name".to_string(), name.to_string())]),
    }
}

/// A BARE cross-file code-entity PLACEHOLDER under `<referencing-file>::<name>` (spec 52's documented
/// shape): no `name` attr, so the files-lens honesty gate (spec 63 c3) cannot take [`file_of`] of its
/// own id directly - that would misattribute it to the REFERENCING file, not its true definition
/// file - and instead resolves it by unique entity-name suffix over the [`ce`] DEFINITIONS in the same
/// graph.
fn bare_ce(id: &str) -> Node {
    Node {
        id: id.to_string(),
        kind: KIND_CODE_ENTITY.to_string(),
        attrs: BTreeMap::new(),
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

/// THE WHOLE-GRAPH FOLD'S OWN "NEVER BLANKED" CONTRACT (spec 63 c3, FILES-LENS PURITY, the sibling
/// fix to [`REPROJECT_FILES_UNRESOLVED`]'s reprojection-cell contract): a NON-EMPTY graph whose Files
/// fold admits NOTHING at all - every node either falls outside `KIND_CODE_ENTITY` or is a bare
/// cross-file placeholder [`clustered_overview`] cannot honestly attribute to one file - must still
/// carry [`WHOLE_GRAPH_FILES_UNRESOLVED`] as its `empty_state`, never a bare `None` that dash.html's
/// `renderKgOverview` would otherwise mistake for a truly empty graph and caption "empty graph -
/// nothing to explore yet" on a graph that plainly is not. This is the IDENTICAL defect class this
/// spec already fixed for the derived lenses' own whole-graph surface (`CODE_LENS_UNDERIVED` /
/// `CONCEPTS_LENS_UNDERIVED` firing on the fold's own post-fold emptiness, not merely on
/// `Buckets::underived()`), extended here to the Files lens's own purity-excluded case. A TRULY empty
/// graph (zero nodes at all) stays `None` - that one degrades to the generic caption correctly.
#[test]
fn clustered_overview_under_files_lens_carries_an_accurate_empty_state_when_the_fold_admits_nothing(
) {
    // Every node here is purity-excluded from the Files fold (spec 63 c3): none is a
    // `KIND_CODE_ENTITY`, so `whole_graph_lens_key` returns `None` for all of them.
    let all_non_code_entities = Graph {
        nodes: vec![
            file_node(FILE_A),
            plain("d1", KIND_DECISION),
            plain("docs/x.md", KIND_DESIGN_DOC),
        ],
        edges: vec![],
    };
    let overview = clustered_overview(&all_non_code_entities, &Lens::Files);
    assert_eq!(
        overview.total, 3,
        "total still counts every node, folded or not"
    );
    assert!(
        overview.clusters.is_empty(),
        "no node here is a code entity, so the fold admits nothing: {overview:?}"
    );
    assert_eq!(
        overview.empty_state.as_deref(),
        Some(WHOLE_GRAPH_FILES_UNRESOLVED),
        "a non-empty graph whose Files fold admits nothing must carry the accurate empty-state \
         caption, never a bare None that reads as a truly empty graph: {overview:?}"
    );

    // Every code entity here IS a bare cross-file placeholder with ZERO matching definitions
    // anywhere in the graph (no `ce(...)` real definition exists at all) - unresolvable honestly, so
    // the fold still admits nothing even though `KIND_CODE_ENTITY` nodes exist. (A real definition
    // would fold under its own file regardless of whether it also candidates for some OTHER bare
    // placeholder's ambiguous resolution - that shape is a different, already-covered test:
    // `clustered_overview_resolves_bare_cross_file_placeholders_by_unique_name_suffix`.)
    let only_unresolvable_placeholders = Graph {
        nodes: vec![
            bare_ce("src/caller.rs::ghost_one"),
            bare_ce("src/caller.rs::ghost_two"),
        ],
        edges: vec![],
    };
    let overview = clustered_overview(&only_unresolvable_placeholders, &Lens::Files);
    assert_eq!(overview.total, 2);
    assert!(
        overview.clusters.is_empty(),
        "every code entity here is a bare placeholder with no definition to resolve to: {overview:?}"
    );
    assert_eq!(
        overview.empty_state.as_deref(),
        Some(WHOLE_GRAPH_FILES_UNRESOLVED),
        "unresolvable bare placeholders leave the fold empty just like non-code-entity nodes, and \
         must carry the same accurate caption: {overview:?}"
    );

    // A TRULY empty graph (no nodes at all) is a DIFFERENT, already-documented degenerate case: it
    // stays `None`, so dash.html's generic "empty graph - nothing to explore yet" caption fires
    // instead - accurate there, unlike the non-empty cases above.
    let empty = Graph {
        nodes: Vec::new(),
        edges: Vec::new(),
    };
    let overview = clustered_overview(&empty, &Lens::Files);
    assert_eq!(overview.total, 0);
    assert_eq!(
        overview.empty_state, None,
        "a truly empty graph carries no empty_state - the generic caption already covers it"
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

/// THE WHOLE-GRAPH FOLD'S OWN RESOLUTION HONESTY (spec 63 c3, FILES-LENS PURITY): a BARE cross-file
/// placeholder (no `name` attr) names the file that REFERENCES it in its own id, never the file that
/// DEFINES it, so [`clustered_overview`] must resolve it by entity-name suffix over the graph's real
/// definitions - the SAME resolution [`reproject`] already performs for the re-projection surface -
/// rather than folding it to the wrong (referencing) file. Three shapes in one graph: a name with
/// EXACTLY ONE definition resolves to that definition's file (and its coupling edge crosses files
/// correctly); a name with NO definition and a name with TWO definitions both cannot be honestly
/// attributed to any one file, so the whole-graph fold EXCLUDES them entirely (there is no
/// `unresolved` sidecar at this altitude, unlike the re-projection surface) - never mis-attributed to
/// the referencing file their own id encodes.
#[test]
fn clustered_overview_resolves_bare_cross_file_placeholders_by_unique_name_suffix() {
    // `helper`: ONE definition (target.rs) - the bare placeholder in caller.rs's namespace resolves
    // to target.rs, so caller.rs's own cluster count stays at just `foo`, target.rs's count rises to
    // 2 (its real definition plus the resolved reference), and the foo->helper coupling now crosses
    // files instead of collapsing to an intra-cluster edge.
    const CALLER_FOO: &str = "src/caller.rs::foo";
    const HELPER_BARE: &str = "src/caller.rs::helper";
    const HELPER_DEF: &str = "src/target.rs::helper";
    // `ghost`: ZERO definitions anywhere - excluded entirely (never attributed to its own referencing
    // file `src/caller.rs`).
    const GHOST_BARE: &str = "src/caller.rs::ghost";
    // `ambiguous`: TWO definitions (x.rs and y.rs) - excluded entirely (never attributed to either
    // candidate file, nor to its own referencing file).
    const AMBIGUOUS_BARE: &str = "src/caller.rs::ambiguous";
    const AMBIGUOUS_DEF_ONE: &str = "src/x.rs::ambiguous";
    const AMBIGUOUS_DEF_TWO: &str = "src/y.rs::ambiguous";

    let graph = Graph {
        nodes: vec![
            ce(CALLER_FOO),
            bare_ce(HELPER_BARE),
            ce(HELPER_DEF),
            bare_ce(GHOST_BARE),
            bare_ce(AMBIGUOUS_BARE),
            ce(AMBIGUOUS_DEF_ONE),
            ce(AMBIGUOUS_DEF_TWO),
        ],
        edges: vec![
            refs(CALLER_FOO, HELPER_BARE),
            refs(CALLER_FOO, GHOST_BARE),
            refs(CALLER_FOO, AMBIGUOUS_BARE),
        ],
    };

    let overview = clustered_overview(&graph, &Lens::Files);
    assert_eq!(
        overview.total, 7,
        "total still counts every node, resolved or excluded"
    );
    assert_eq!(
        overview.clusters,
        vec![
            Cluster {
                key: "src/caller.rs".to_string(),
                count: 1,
                kind: KIND_CODE_ENTITY.to_string(),
                label: None,
            },
            Cluster {
                key: "src/target.rs".to_string(),
                count: 2,
                kind: KIND_CODE_ENTITY.to_string(),
                label: None,
            },
            Cluster {
                key: "src/x.rs".to_string(),
                count: 1,
                kind: KIND_CODE_ENTITY.to_string(),
                label: None,
            },
            Cluster {
                key: "src/y.rs".to_string(),
                count: 1,
                kind: KIND_CODE_ENTITY.to_string(),
                label: None,
            },
        ],
        "caller.rs keeps only its real `foo` (the resolved `helper` reference moves to target.rs, \
         raising ITS count to 2); the ghost and ambiguous placeholders fold to no cluster at all - \
         never to caller.rs, never to x.rs/y.rs: {overview:?}"
    );
    assert_eq!(
        overview.edges,
        vec![ClusterEdge {
            from: "src/caller.rs".to_string(),
            to: "src/target.rs".to_string(),
            weight: 1,
        }],
        "the foo->helper coupling now correctly crosses to target.rs (the TRUE definition's file), \
         not caller.rs (the referencing file the bare id encodes); the ghost and ambiguous edges add \
         no cluster edge since their far endpoint folds to nothing: {overview:?}"
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
