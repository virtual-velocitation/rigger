//! Periphery (API / integration / contract) tests for spec 55's SUBJECT x LENS RE-PROJECTION
//! (criterion 1): the `seed=<id>&lens=<files|code|concepts>` composition on the dash's `/api/graph`
//! route. Selecting a SUBJECT and flipping the LENS re-grains THAT subject's member set at the
//! chosen altitude - it does NOT switch to a whole-graph overview. The projection rule is: resolve
//! the subject's MEMBER SET at its own grain (a concept's `REALIZES` members; a community's members;
//! a file's contained entities; a single entity is its own set), then re-bucket that set under the
//! requested lens. So a concept under `lens=code` shows its members grouped by coupling community,
//! and under `lens=files` shows the DISTINCT FILES those members resolve to.
//!
//! This criterion OWNS the re-projection MECHANICS and its RESOLUTION HONESTY:
//!
//!  - CROSS-GRAIN RESOLUTION never trusts the id. Projecting members to files resolves a BARE
//!    cross-file placeholder node (a call target in the CALLER's namespace, carrying no `name` attr)
//!    to its DEFINING file by the pinned name-suffix match: EXACTLY ONE definition auto-resolves to
//!    that definition's file; MORE THAN ONE (or zero) surfaces as a MARKED-UNRESOLVED entry carrying
//!    the sorted candidate definition ids - never a wrong attribution to the referencing file the id
//!    encodes.
//!  - DETERMINISM. Member sets, file buckets, community buckets, and the unresolved list all sort
//!    deterministically, so the same graph + subject + lens yield a byte-identical body.
//!  - COMPOSITION-ABSENT BACK-COMPAT. A non-empty `seed` with NO `lens` param is the byte-identical
//!    spec-30 seeded neighborhood (a node-and-edge walk), never the re-projection - so the extension
//!    never regresses the existing seeded panel.
//!
//! These run OUTSIDE the crate, over the library's PUBLIC surface (`rigger::dash::{reproject,
//! Reprojection, UnresolvedMember, Cluster, ClusterEdge, route, Lens}`), so they guard the exact
//! public boundary a same-crate `super::` test is structurally blind to, and drive the served
//! `route` end-to-end through the `seed=` / `lens=` / `resolution=` query parsing the browser hits.

use std::collections::HashMap;

use rigger::contextgraph::{
    Edge, Graph, Node, KIND_CODE_ENTITY, KIND_COMMUNITY, KIND_CONCEPT, REL_CALLS, REL_IN_COMMUNITY,
    REL_REALIZES, TIER_EXTRACTED, TIER_INFERRED,
};
use rigger::dash::{reproject, route, Cluster, ClusterEdge, Lens, UnresolvedMember};

// The fixture concept the whole test re-grains, and its two coupling communities.
const CONCEPT: &str = "concept/1/0";
const C0: &str = "community/1/0";
const C1: &str = "community/1/1";

// The concept's DEFINITION members: real code-entity definitions (each carries a `name` attr), one
// per file, split across two communities so the code re-grain crosses directory lines.
const FOO: &str = "src/alpha/a.rs::foo";
const BAR: &str = "src/beta/b.rs::bar";
const BAZ: &str = "src/gamma/c.rs::baz";

// The concept's BARE placeholder members: call targets sitting in a.rs's namespace with NO `name`
// attr (the definition lives elsewhere). `HELPER` resolves to a SINGLE definition file; `RUN`'s name
// has TWO definitions, so it is marked-unresolved rather than attributed to a wrong file.
const HELPER_BARE: &str = "src/alpha/a.rs::helper";
const RUN_BARE: &str = "src/alpha/a.rs::run";

// The standalone DEFINITION nodes the bare members resolve against (NOT members of the concept).
const HELPER_DEF: &str = "src/delta/d.rs::helper";
const RUN_DEF_ONE: &str = "src/one/x.rs::run";
const RUN_DEF_TWO: &str = "src/two/y.rs::run";

/// A code-entity DEFINITION node: carries the `name` attr that marks it a real definition (not a bare
/// cross-file placeholder), exactly as the extraction fold records.
fn def(id: &str, name: &str) -> Node {
    let mut n = Node {
        id: id.to_string(),
        kind: KIND_CODE_ENTITY.to_string(),
        attrs: Default::default(),
    };
    n.attrs.insert("name".to_string(), name.to_string());
    n
}

/// A BARE cross-file placeholder code-entity: NO `name` attr, so cross-grain file resolution must
/// resolve it by name-suffix to its defining file rather than trust its (referencing-file) id.
fn bare(id: &str) -> Node {
    Node {
        id: id.to_string(),
        kind: KIND_CODE_ENTITY.to_string(),
        attrs: Default::default(),
    }
}

/// A derived super-node (`KIND_CONCEPT` / `KIND_COMMUNITY`) carrying its deterministic display
/// `label`. It is a BUCKET, never a member, so it is excluded from every count.
fn super_node(id: &str, kind: &str, label: &str) -> Node {
    let mut n = Node {
        id: id.to_string(),
        kind: kind.to_string(),
        attrs: Default::default(),
    };
    n.attrs.insert("label".to_string(), label.to_string());
    n
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

/// The re-projection fixture. The concept `concept/1/0` REALIZES five members: three definitions
/// (foo/bar/baz) across three files and two communities, plus two BARE placeholders (helper, run) in
/// a.rs's namespace. helper's name has ONE definition (d.rs::helper), run's name has TWO (x.rs::run,
/// y.rs::run). Every member carries a community membership so the CODE re-grain is pure community
/// buckets. The standalone definition nodes the bare members resolve against are present but are NOT
/// members. One cross-community CALLS edge (foo->baz) exercises the cross-bucket coupling fold.
fn reproj_graph() -> Graph {
    Graph {
        nodes: vec![
            super_node(CONCEPT, KIND_CONCEPT, "the pipeline"),
            super_node(C0, KIND_COMMUNITY, "foo"),
            super_node(C1, KIND_COMMUNITY, "baz"),
            def(FOO, "foo"),
            def(BAR, "bar"),
            def(BAZ, "baz"),
            bare(HELPER_BARE),
            bare(RUN_BARE),
            // Standalone definitions the bare members resolve against (not concept members).
            def(HELPER_DEF, "helper"),
            def(RUN_DEF_ONE, "run"),
            def(RUN_DEF_TWO, "run"),
        ],
        edges: vec![
            // The concept's REALIZES membership: its five members.
            edge(FOO, CONCEPT, REL_REALIZES, TIER_EXTRACTED),
            edge(BAR, CONCEPT, REL_REALIZES, TIER_EXTRACTED),
            edge(BAZ, CONCEPT, REL_REALIZES, TIER_EXTRACTED),
            edge(HELPER_BARE, CONCEPT, REL_REALIZES, TIER_EXTRACTED),
            edge(RUN_BARE, CONCEPT, REL_REALIZES, TIER_EXTRACTED),
            // Community memberships (grain 1): foo/bar/helper -> C0, baz/run -> C1.
            edge(FOO, C0, REL_IN_COMMUNITY, TIER_INFERRED),
            edge(BAR, C0, REL_IN_COMMUNITY, TIER_INFERRED),
            edge(HELPER_BARE, C0, REL_IN_COMMUNITY, TIER_INFERRED),
            edge(BAZ, C1, REL_IN_COMMUNITY, TIER_INFERRED),
            edge(RUN_BARE, C1, REL_IN_COMMUNITY, TIER_INFERRED),
            // One cross-community coupling edge among members.
            edge(FOO, BAZ, REL_CALLS, TIER_EXTRACTED),
        ],
    }
}

/// THE CODE RE-GRAIN over the public boundary: `reproject(graph, concept, &Lens::Code)` re-grains the
/// concept's MEMBER SET (not the whole graph) by coupling community - foo/bar/helper into C0, baz/run
/// into C1 - each community sized by MEMBER count, labelled by its deterministic label, and the one
/// cross-community coupling edge folded to a symmetric super-edge. `total` is the member-set size, not
/// the whole graph.
#[test]
fn concept_recodegrains_its_members_by_coupling_community() {
    let re = reproject(&reproj_graph(), CONCEPT, &code_lens());

    assert_eq!(re.subject, CONCEPT, "the re-projection echoes its subject");
    assert_eq!(
        re.total, 5,
        "total is the concept's MEMBER-SET size (5 members), not the whole graph"
    );
    assert!(
        re.unresolved.is_empty(),
        "the CODE re-grain has no file-resolution, so no unresolved entries: {re:?}"
    );
    assert_eq!(
        re.clusters,
        vec![
            Cluster {
                key: C0.to_string(),
                count: 3,
                kind: KIND_CODE_ENTITY.to_string(),
                label: Some("foo".to_string()),
            },
            Cluster {
                key: C1.to_string(),
                count: 2,
                kind: KIND_CODE_ENTITY.to_string(),
                label: Some("baz".to_string()),
            },
        ],
        "the concept's members re-grain into their two communities, sized by member count and labelled: {re:?}"
    );
    assert_eq!(
        re.edges,
        vec![ClusterEdge {
            from: C0.to_string(),
            to: C1.to_string(),
            weight: 1,
        }],
        "the one cross-community coupling edge (foo->baz) folds to a symmetric super-edge: {re:?}"
    );
}

/// THE FILES RE-GRAIN over the public boundary, the RESOLUTION HONESTY the criterion owns:
/// `reproject(graph, concept, &Lens::Files)` shows the DISTINCT DEFINING FILES the members resolve to.
///   * a DEFINITION member (foo/bar/baz) resolves to its own file;
///   * a BARE member whose name has ONE definition (helper) resolves to that DEFINITION's file
///     (d.rs) - NOT the referencing file (a.rs) its id encodes;
///   * a BARE member whose name has TWO definitions (run) is MARKED-UNRESOLVED with its sorted
///     candidate ids, never folded into a wrong file bucket.
#[test]
fn concept_files_regrain_resolves_bare_members_to_their_defining_files() {
    let re = reproject(&reproj_graph(), CONCEPT, &Lens::Files);

    assert_eq!(re.subject, CONCEPT, "the re-projection echoes its subject");
    assert_eq!(
        re.total, 5,
        "total is the concept's MEMBER-SET size (5 members), resolved or not"
    );
    assert_eq!(
        re.clusters,
        vec![
            // foo -> its own file; bar -> its own file; the bare helper -> its DEFINING file (d.rs,
            // not the referencing a.rs); baz -> its own file. Sorted by file key.
            file_bucket("src/alpha/a.rs"),
            file_bucket("src/beta/b.rs"),
            file_bucket("src/delta/d.rs"),
            file_bucket("src/gamma/c.rs"),
        ],
        "each resolvable member folds under its DISTINCT DEFINING file (the bare helper resolved to d.rs): {re:?}"
    );
    assert_eq!(
        re.unresolved,
        vec![UnresolvedMember {
            id: RUN_BARE.to_string(),
            candidates: vec![RUN_DEF_ONE.to_string(), RUN_DEF_TWO.to_string()],
        }],
        "the multi-candidate bare member (run) is marked-unresolved with its sorted candidates, never attributed to a wrong file: {re:?}"
    );
    // The honesty rule, stated as an exclusion: neither candidate file for the ambiguous `run` is a
    // bucket (it is not attributed to either), and a.rs's bucket holds ONLY foo (not the bare run).
    let keys: Vec<&str> = re.clusters.iter().map(|c| c.key.as_str()).collect();
    assert!(
        !keys.contains(&"src/one/x.rs") && !keys.contains(&"src/two/y.rs"),
        "an ambiguous bare member is attributed to NEITHER candidate file: {re:?}"
    );
    assert_eq!(
        re.clusters.iter().find(|c| c.key == "src/alpha/a.rs").map(|c| c.count),
        Some(1),
        "a.rs holds only its definition member foo, never the bare run it merely references: {re:?}"
    );
}

/// THE SERVED `/api/graph` ROUTE threads the `seed=` + `lens=` composition END-TO-END: a non-empty
/// seed WITH an explicit lens re-grains that subject (never a whole-graph overview), while a seed with
/// NO lens stays the byte-identical spec-30 seeded neighborhood (the composition-absent back-compat).
#[test]
fn the_served_graph_route_reprojects_a_seed_under_a_lens() {
    // --- seed + lens=code via the route: the concept's community re-grain ---
    let code = served_json("/api/graph?seed=concept%2F1%2F0&lens=code&resolution=1");
    let keys: Vec<&str> = code["clusters"]
        .as_array()
        .expect("clusters array")
        .iter()
        .map(|c| c["key"].as_str().expect("cluster key is a string"))
        .collect();
    assert_eq!(
        keys,
        vec![C0, C1],
        "the served code re-grain buckets the concept's members by community: {code}"
    );
    assert_eq!(
        code["subject"].as_str(),
        Some(CONCEPT),
        "the served re-projection echoes its subject: {code}"
    );

    // --- seed + lens=files via the route: distinct defining files + the unresolved sidecar ---
    let files = served_json("/api/graph?seed=concept%2F1%2F0&lens=files");
    let file_keys: Vec<&str> = files["clusters"]
        .as_array()
        .expect("clusters array")
        .iter()
        .map(|c| c["key"].as_str().expect("cluster key is a string"))
        .collect();
    assert_eq!(
        file_keys,
        vec!["src/alpha/a.rs", "src/beta/b.rs", "src/delta/d.rs", "src/gamma/c.rs"],
        "the served files re-grain buckets by distinct defining file (bare helper resolved to d.rs): {files}"
    );
    assert_eq!(
        files["unresolved"][0]["id"].as_str(),
        Some(RUN_BARE),
        "the served files re-grain surfaces the ambiguous bare member as unresolved: {files}"
    );

    // --- COMPOSITION ABSENT: a seed with NO lens is the seeded neighborhood, not a re-projection ---
    let neighborhood = served_json("/api/graph?seed=concept%2F1%2F0");
    assert_eq!(
        neighborhood["seed"].as_str(),
        Some(CONCEPT),
        "a lens-absent seed request is the spec-30 seeded neighborhood, echoing its seed: {neighborhood}"
    );
    assert!(
        neighborhood.get("clusters").is_none() && neighborhood.get("subject").is_none(),
        "a lens-absent seed request carries NO re-projection shape (no clusters, no subject key): {neighborhood}"
    );
    assert!(
        neighborhood.get("nodes").is_some(),
        "a lens-absent seed request carries the neighborhood node set: {neighborhood}"
    );
}

// --- helpers ------------------------------------------------------------------------------------

/// The default code lens (`resolution = 1`).
fn code_lens() -> Lens {
    Lens::from_query(Some("code"), Some("1"))
}

/// A single-definition file bucket (one code-entity member, dominant kind code-entity, no label).
fn file_bucket(file: &str) -> Cluster {
    Cluster {
        key: file.to_string(),
        count: 1,
        kind: KIND_CODE_ENTITY.to_string(),
        label: None,
    }
}

/// Drive the public `route` for `GET <target>` over the fixture and parse the body as JSON.
fn served_json(target: &str) -> serde_json::Value {
    let graph = reproj_graph();
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
        "GET {target} must be served 200 (the re-projection route never errors on a live graph)"
    );
    serde_json::from_slice(&resp.body)
        .unwrap_or_else(|e| panic!("the served {target} body must be valid JSON: {e}"))
}
