//! Contract / API periphery tests for spec 55 criterion 1's SUBJECT x LENS RE-PROJECTION, covering
//! the arms of the public `reproject` boundary that the mechanics-and-honesty periphery suite
//! (`tests/subject_lens_reprojection_periphery.rs`) does not exercise. Those tests drive a CONCEPT
//! subject under the Code and Files lenses; the public `reproject(graph, subject, &Lens)` contract is
//! wider on TWO independent axes, and each axis is a documented dispatch arm of the same function:
//!
//!  - the LENS axis: `Lens::Concepts` re-buckets a member set by its derived CONCEPT (the third fold,
//!    alongside Code's community fold and Files' defining-file fold). A membership-less member keeps
//!    its KIND bucket, so a re-projection stays whole (nothing silently dropped);
//!  - the SUBJECT axis: `member_set` dispatches on the subject's OWN node kind - a COMMUNITY re-grains
//!    to its `IN_COMMUNITY` members, a FILE to its `CONTAINS` entities, a single entity is its own
//!    singleton set, and an UNKNOWN subject is the empty set (the empty cell the mechanics ship);
//!  - the SERIALIZED FORM: `Reprojection` is the JSON body the browser consumes, and its
//!    `#[serde(skip_serializing_if)]` on `unresolved` is a wire contract - the key is ABSENT when the
//!    list is empty (a lean body) and PRESENT when the honesty rule marked a member, and the body is
//!    byte-identical across two folds of the same input (the determinism the panel relies on);
//!  - the ROUTE PRECEDENCE: the `seed=`+`lens=` re-projection is dispatched only when NO `cluster=` is
//!    present, so a `cluster=` drill still drills (the c1 dispatch insertion never hijacks it), and a
//!    NON-concept subject re-projects over the served route exactly as a concept does.
//!
//! These run OUTSIDE the crate over the library's PUBLIC surface (`rigger::dash::{reproject,
//! Reprojection, route, Cluster, Lens}`), so they guard the exact public boundary a same-crate
//! `super::` test is structurally blind to, and drive the served `route` end-to-end.

use std::collections::HashMap;

use rigger::contextgraph::{
    Edge, Graph, Node, KIND_CODE_ENTITY, KIND_COMMUNITY, KIND_CONCEPT, KIND_FILE, REL_CONTAINS,
    REL_IN_COMMUNITY, REL_REALIZES,
};
use rigger::dash::{reproject, route, Cluster, Lens};

// --- fixture helpers ----------------------------------------------------------------------------

/// A code-entity DEFINITION node: carries the `name` attr that marks it a real definition, exactly as
/// the extraction fold records. Under the files re-grain a `name`-bearing node folds under its OWN
/// file rather than resolving by name-suffix.
fn def(id: &str, name: &str) -> Node {
    let mut n = Node {
        id: id.to_string(),
        kind: KIND_CODE_ENTITY.to_string(),
        attrs: Default::default(),
    };
    n.attrs.insert("name".to_string(), name.to_string());
    n
}

/// A BARE cross-file placeholder code-entity: NO `name` attr, so a files re-grain resolves it by
/// name-suffix to its defining file(s) rather than trusting its referencing-file id.
fn bare(id: &str) -> Node {
    Node {
        id: id.to_string(),
        kind: KIND_CODE_ENTITY.to_string(),
        attrs: Default::default(),
    }
}

/// A plain node of a given kind (a `file` subject, or a `community` / `concept` super-node). A
/// super-node optionally carries its deterministic display `label`.
fn node(id: &str, kind: &str, label: Option<&str>) -> Node {
    let mut n = Node {
        id: id.to_string(),
        kind: kind.to_string(),
        attrs: Default::default(),
    };
    if let Some(l) = label {
        n.attrs.insert("label".to_string(), l.to_string());
    }
    n
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
        tier: "extracted".to_string(),
    }
}

/// The default concepts lens (`resolution = 1`).
fn concepts_lens() -> Lens {
    Lens::from_query(Some("concepts"), Some("1"))
}

/// The default code lens (`resolution = 1`).
fn code_lens() -> Lens {
    Lens::from_query(Some("code"), Some("1"))
}

/// A code-entity file bucket (dominant kind code-entity), sized `count`, with an optional super-node
/// `label`.
fn bucket(key: &str, count: usize, label: Option<&str>) -> Cluster {
    Cluster {
        key: key.to_string(),
        count,
        kind: KIND_CODE_ENTITY.to_string(),
        label: label.map(str::to_string),
    }
}

/// Drive the public `route` for `GET <target>` over `graph` and parse the body as JSON.
fn served_json(graph: &Graph, target: &str) -> serde_json::Value {
    let liveness: HashMap<String, u64> = HashMap::new();
    let resp = route(
        "GET",
        target,
        &[],
        graph,
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

// --- the LENS axis: the Concepts fold, on a COMMUNITY subject --------------------------------------

const COMMUNITY: &str = "community/1/9";
const CONCEPT_A: &str = "concept/1/0";
const CONCEPT_B: &str = "concept/1/1";

/// A community whose four members re-grain by CONCEPT: two members realize concept A, one realizes
/// concept B, and one realizes NOTHING (so it keeps its kind bucket). This exercises BOTH the
/// concepts fold (the lens arm the mechanics suite omits) AND a community subject's `IN_COMMUNITY`
/// member resolution.
fn community_over_concepts_graph() -> Graph {
    Graph {
        nodes: vec![
            node(COMMUNITY, KIND_COMMUNITY, Some("the subsystem")),
            node(CONCEPT_A, KIND_CONCEPT, Some("ingest")),
            node(CONCEPT_B, KIND_CONCEPT, Some("render")),
            def("src/a/m1.rs::m1", "m1"),
            def("src/a/m2.rs::m2", "m2"),
            def("src/b/m3.rs::m3", "m3"),
            def("src/c/m4.rs::m4", "m4"),
        ],
        edges: vec![
            // The community's IN_COMMUNITY membership: its four members.
            edge("src/a/m1.rs::m1", COMMUNITY, REL_IN_COMMUNITY),
            edge("src/a/m2.rs::m2", COMMUNITY, REL_IN_COMMUNITY),
            edge("src/b/m3.rs::m3", COMMUNITY, REL_IN_COMMUNITY),
            edge("src/c/m4.rs::m4", COMMUNITY, REL_IN_COMMUNITY),
            // REALIZES memberships: m1/m2 -> concept A, m3 -> concept B, m4 -> none.
            edge("src/a/m1.rs::m1", CONCEPT_A, REL_REALIZES),
            edge("src/a/m2.rs::m2", CONCEPT_A, REL_REALIZES),
            edge("src/b/m3.rs::m3", CONCEPT_B, REL_REALIZES),
        ],
    }
}

/// THE CONCEPTS RE-GRAIN over the public boundary, on a COMMUNITY subject:
/// `reproject(graph, community, &Lens::Concepts)` re-grains the community's `IN_COMMUNITY` member set
/// (not the whole graph) by derived CONCEPT - m1/m2 into concept A (labelled), m3 into concept B
/// (labelled), and the concept-less m4 into its KIND bucket (never dropped). `total` is the member-set
/// size; the concept super-nodes are themselves excluded from the fold.
#[test]
fn community_subject_regrains_its_members_by_concept_under_the_concepts_lens() {
    let re = reproject(
        &community_over_concepts_graph(),
        COMMUNITY,
        &concepts_lens(),
    );

    assert_eq!(
        re.subject, COMMUNITY,
        "the re-projection echoes its subject"
    );
    assert_eq!(
        re.total, 4,
        "total is the community's MEMBER-SET size (4 members), not the whole graph"
    );
    assert!(
        re.unresolved.is_empty(),
        "a DERIVED-lens re-grain never resolves files, so it carries no unresolved entries: {re:?}"
    );
    assert_eq!(
        re.clusters,
        vec![
            // m4 realizes no concept, so it keeps its KIND bucket - the re-grain stays whole.
            bucket(KIND_CODE_ENTITY, 1, None),
            // The two concept buckets, sized by member count and labelled by their concept.
            bucket(CONCEPT_A, 2, Some("ingest")),
            bucket(CONCEPT_B, 1, Some("render")),
        ],
        "the community's members re-grain by concept, the concept-less member keeps its kind bucket: {re:?}"
    );
    assert!(
        re.edges.is_empty(),
        "no cross-concept coupling edge among the members, so no super-edge: {re:?}"
    );
}

// --- the SUBJECT axis: a FILE subject --------------------------------------------------------------

const FILE_SUBJECT: &str = "src/pkg/mod.rs";
const COMM_ALPHA: &str = "community/1/0";
const COMM_BETA: &str = "community/1/1";

/// A file subject whose three CONTAINED entities re-grain by coupling community: two into alpha, one
/// into beta.
fn file_over_code_graph() -> Graph {
    Graph {
        nodes: vec![
            node(FILE_SUBJECT, KIND_FILE, None),
            node(COMM_ALPHA, KIND_COMMUNITY, Some("alpha")),
            node(COMM_BETA, KIND_COMMUNITY, Some("beta")),
            def("src/pkg/mod.rs::e1", "e1"),
            def("src/pkg/mod.rs::e2", "e2"),
            def("src/pkg/mod.rs::e3", "e3"),
        ],
        edges: vec![
            // The file's CONTAINS membership: its three entities.
            edge(FILE_SUBJECT, "src/pkg/mod.rs::e1", REL_CONTAINS),
            edge(FILE_SUBJECT, "src/pkg/mod.rs::e2", REL_CONTAINS),
            edge(FILE_SUBJECT, "src/pkg/mod.rs::e3", REL_CONTAINS),
            // Community memberships: e1/e2 -> alpha, e3 -> beta.
            edge("src/pkg/mod.rs::e1", COMM_ALPHA, REL_IN_COMMUNITY),
            edge("src/pkg/mod.rs::e2", COMM_ALPHA, REL_IN_COMMUNITY),
            edge("src/pkg/mod.rs::e3", COMM_BETA, REL_IN_COMMUNITY),
        ],
    }
}

/// A FILE subject re-grains to the entities it CONTAINS (the `member_set` file arm), re-bucketed under
/// the requested lens: `reproject(graph, file, &Lens::Code)` groups the file's contained entities by
/// coupling community, sized by member count and labelled - proving the subject axis dispatches on the
/// subject's OWN kind, not only a concept.
#[test]
fn file_subject_regrains_its_contained_entities_by_community_under_the_code_lens() {
    let re = reproject(&file_over_code_graph(), FILE_SUBJECT, &code_lens());

    assert_eq!(
        re.subject, FILE_SUBJECT,
        "the re-projection echoes the file subject"
    );
    assert_eq!(
        re.total, 3,
        "total is the file's CONTAINED-entity count (3), not the whole graph"
    );
    assert_eq!(
        re.clusters,
        vec![
            bucket(COMM_ALPHA, 2, Some("alpha")),
            bucket(COMM_BETA, 1, Some("beta")),
        ],
        "the file's contained entities re-grain into their two communities, sized and labelled: {re:?}"
    );
    assert!(
        re.unresolved.is_empty(),
        "a code re-grain has no file resolution: {re:?}"
    );
}

// --- the SUBJECT axis: a single-entity subject, and an unknown subject -----------------------------

const SOLO: &str = "src/x.rs::solo";

/// A single entity (neither concept, community, nor file) is its OWN singleton member set: under the
/// files lens it folds to its one defining file, and under a derived lens with no membership it keeps
/// its kind bucket. So flipping the lens on a leaf never empties the panel - the instrument composes
/// down to a single node.
#[test]
fn single_entity_subject_is_its_own_member_set() {
    let graph = Graph {
        nodes: vec![def(SOLO, "solo")],
        edges: vec![],
    };

    let files = reproject(&graph, SOLO, &Lens::Files);
    assert_eq!(
        files.subject, SOLO,
        "the singleton re-projection echoes its subject"
    );
    assert_eq!(
        files.total, 1,
        "a single entity is a member set of exactly one"
    );
    assert_eq!(
        files.clusters,
        vec![bucket("src/x.rs", 1, None)],
        "under files the lone entity folds to its one defining file: {files:?}"
    );
    assert!(
        files.unresolved.is_empty(),
        "a named definition needs no resolution: {files:?}"
    );

    // Under a DERIVED lens the membership-less lone entity keeps its KIND bucket (never dropped).
    let code = reproject(&graph, SOLO, &code_lens());
    assert_eq!(
        code.clusters,
        vec![bucket(KIND_CODE_ENTITY, 1, None)],
        "under code a membership-less lone entity keeps its kind bucket: {code:?}"
    );
    assert_eq!(code.total, 1, "the member-set size is still one");
}

/// An UNKNOWN subject (absent from the graph) has an EMPTY member set, so `reproject` returns an empty
/// body - the subject echoed, zero clusters, zero edges, zero total, no unresolved - rather than
/// panicking or leaking a whole-graph overview. This is the c1 mechanics of the documented empty cell
/// (the presentation message is c2's additive extension, not asserted here).
#[test]
fn unknown_subject_yields_an_empty_reprojection() {
    // A non-trivial graph, to prove the empty result is the SUBJECT's empty member set, not an empty
    // graph.
    let graph = file_over_code_graph();

    for lens in [Lens::Files, code_lens(), concepts_lens()] {
        let re = reproject(&graph, "no/such/subject", &lens);
        assert_eq!(
            re.subject, "no/such/subject",
            "the empty cell still echoes its subject: {re:?}"
        );
        assert_eq!(
            re.total, 0,
            "an unknown subject has a zero-size member set: {re:?}"
        );
        assert!(
            re.clusters.is_empty(),
            "an unknown subject folds to no clusters: {re:?}"
        );
        assert!(
            re.edges.is_empty(),
            "an unknown subject folds to no edges: {re:?}"
        );
        assert!(
            re.unresolved.is_empty(),
            "an unknown subject marks nothing unresolved: {re:?}"
        );
    }
}

// --- the SERIALIZED FORM: the skip_serializing_if wire contract + determinism ----------------------

/// A concept whose one member is a BARE placeholder resolving to TWO definitions - so a files re-grain
/// marks it unresolved and the JSON body carries a non-empty `unresolved`.
fn ambiguous_files_graph() -> Graph {
    Graph {
        nodes: vec![
            node("concept/9/0", KIND_CONCEPT, Some("the idea")),
            bare("src/caller.rs::amb"),
            def("src/p/x.rs::amb", "amb"),
            def("src/q/y.rs::amb", "amb"),
        ],
        edges: vec![edge("src/caller.rs::amb", "concept/9/0", REL_REALIZES)],
    }
}

/// The `Reprojection` JSON body honours its `#[serde(skip_serializing_if)]` wire contract: `unresolved`
/// is OMITTED when empty (a lean body for the common resolvable re-grain) and PRESENT as an array of
/// `{id, candidates}` when the honesty rule marked a member. The stable fields (subject / clusters /
/// edges / total) are always present, and the body is byte-identical across two folds of the same
/// input (the determinism the spec requires by construction).
#[test]
fn reprojection_json_omits_unresolved_when_empty_and_carries_it_when_present() {
    // --- EMPTY: a fully-resolvable code re-grain drops the `unresolved` key entirely ---
    let resolvable = reproject(&file_over_code_graph(), FILE_SUBJECT, &code_lens());
    let body = serde_json::to_value(&resolvable).expect("Reprojection serializes to JSON");
    let obj = body
        .as_object()
        .expect("a Reprojection serializes to a JSON object");
    assert!(
        !obj.contains_key("unresolved"),
        "an empty unresolved list is SKIPPED from the JSON, keeping the body lean: {body}"
    );
    for key in ["subject", "clusters", "edges", "total"] {
        assert!(
            obj.contains_key(key),
            "the body always carries `{key}`: {body}"
        );
    }

    // Determinism: two independent folds of the same input serialize to byte-identical bodies.
    let again = reproject(&file_over_code_graph(), FILE_SUBJECT, &code_lens());
    assert_eq!(
        serde_json::to_string(&resolvable).unwrap(),
        serde_json::to_string(&again).unwrap(),
        "the same graph + subject + lens yield a byte-identical body"
    );

    // --- PRESENT: an ambiguous bare member surfaces `unresolved` as an array of {id, candidates} ---
    let marked = reproject(&ambiguous_files_graph(), "concept/9/0", &Lens::Files);
    let body = serde_json::to_value(&marked).expect("Reprojection serializes to JSON");
    let obj = body
        .as_object()
        .expect("a Reprojection serializes to a JSON object");
    let unresolved = obj
        .get("unresolved")
        .expect("a marked re-grain carries the `unresolved` key")
        .as_array()
        .expect("`unresolved` serializes as an array");
    assert_eq!(
        unresolved.len(),
        1,
        "exactly the one ambiguous member is marked: {body}"
    );
    assert_eq!(
        unresolved[0]["id"].as_str(),
        Some("src/caller.rs::amb"),
        "the marked member carries its bare id, never a wrong file attribution: {body}"
    );
    assert_eq!(
        unresolved[0]["candidates"]
            .as_array()
            .expect("candidates is an array")
            .iter()
            .map(|c| c.as_str().expect("a candidate id is a string"))
            .collect::<Vec<_>>(),
        vec!["src/p/x.rs::amb", "src/q/y.rs::amb"],
        "the sorted candidate definition ids are the re-seed frontier: {body}"
    );
}

// --- the ROUTE precedence: cluster= drill wins, and a non-concept subject re-projects served --------

/// The served `/api/graph` dispatch: a `seed=`+`lens=` request re-projects even a NON-concept (file)
/// subject end-to-end, while a `cluster=` request still DRILLS - the c1 re-projection branch is
/// dispatched only in the no-`cluster` path, so it never hijacks a drill.
#[test]
fn served_reproject_fires_for_a_file_subject_and_cluster_drill_takes_precedence() {
    let graph = file_over_code_graph();

    // A file seed under a lens re-projects (the served body is a Reprojection: subject + clusters).
    let reproj = served_json(&graph, "/api/graph?seed=src%2Fpkg%2Fmod.rs&lens=code");
    assert_eq!(
        reproj["subject"].as_str(),
        Some(FILE_SUBJECT),
        "a served file-subject re-projection echoes its subject: {reproj}"
    );
    let keys: Vec<&str> = reproj["clusters"]
        .as_array()
        .expect("clusters array")
        .iter()
        .map(|c| c["key"].as_str().expect("cluster key is a string"))
        .collect();
    assert_eq!(
        keys,
        vec![COMM_ALPHA, COMM_BETA],
        "the served file re-grain buckets its contained entities by community: {reproj}"
    );

    // A cluster drill with a lens present STILL drills (a Neighborhood: seed + nodes, never a
    // re-projection's subject/clusters shape) - the re-projection branch never fires under `cluster=`.
    let drill = served_json(&graph, "/api/graph?cluster=community%2F1%2F0&lens=code");
    assert_eq!(
        drill["seed"].as_str(),
        Some(COMM_ALPHA),
        "a cluster= request drills that cluster (a seeded neighborhood keyed by the cluster): {drill}"
    );
    assert!(
        drill.get("nodes").is_some(),
        "the drill carries the cluster's member neighborhood: {drill}"
    );
    assert!(
        drill.get("subject").is_none() && drill.get("clusters").is_none(),
        "a cluster= drill is NOT a re-projection: the c1 branch never hijacks it: {drill}"
    );
}
