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
    Edge, Graph, Node, KIND_CODE_ENTITY, KIND_COMMUNITY, KIND_CONCEPT, KIND_DECISION, KIND_FILE,
    REL_CONTAINS, REL_GOVERNS, REL_IN_COMMUNITY, REL_REALIZES,
};
use rigger::dash::{reproject, route, Cluster, Lens, UnresolvedMember};

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

/// A decision node carrying the `summary` attr the rationale fold reads (the CONTENT the overlay
/// echoes), so a `GOVERNS`/`ABOUT` edge from it into a node makes that node carry a rationale leaf.
fn decision(id: &str, summary: &str) -> Node {
    let mut n = Node {
        id: id.to_string(),
        kind: KIND_DECISION.to_string(),
        attrs: Default::default(),
    };
    n.attrs.insert("summary".to_string(), summary.to_string());
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

// --- the FILES honesty sentinels: a ZERO-candidate bare member + a NO-file-identity member ----------
//
// `reproject_files` resolves a BARE placeholder by name-suffix over the definitions: EXACTLY ONE
// auto-resolves and MORE THAN ONE is marked-unresolved with its candidates - both arms are covered by
// the mechanics periphery (`helper` -> one, `run` -> two) and the serialize contract above. TWO
// resolution arms remain that no fixture in either suite hits, yet each is a documented honesty
// guarantee ("nothing silently dropped") that a regression would break green:
//  - the ZERO-candidate bare member: a call target whose name matches NO definition anywhere (an
//    external / not-yet-extracted symbol - an EVERYDAY case). It must be marked unresolved with an
//    EMPTY candidate frontier, never dropped and never mis-attributed to the file its id references;
//  - the NO-file-identity member: a non-bare member whose id names no file (a non-code singleton
//    subject, e.g. a decision node). Under files it keeps its KIND bucket - the files-lens twin of the
//    derived-lens kind fallback the single-entity test proves for `code`.

const UNMATCHED_CONCEPT: &str = "concept/7/0";
const EXTERNAL_BARE: &str = "src/caller.rs::external_symbol";

/// A concept whose one member is a bare placeholder whose name matches NO definition anywhere.
fn unmatched_bare_graph() -> Graph {
    Graph {
        nodes: vec![
            node(UNMATCHED_CONCEPT, KIND_CONCEPT, Some("the idea")),
            bare(EXTERNAL_BARE),
        ],
        edges: vec![edge(EXTERNAL_BARE, UNMATCHED_CONCEPT, REL_REALIZES)],
    }
}

/// A bare member whose name resolves to ZERO definitions is marked-unresolved with an EMPTY candidate
/// frontier - the honest "unknown" signal (a call to an external / not-yet-extracted symbol). It is
/// counted in `total`, folds to NO file bucket, and is NEVER attributed to the referencing file its id
/// encodes; the empty frontier crosses the wire as `[]`, not an omitted key.
#[test]
fn a_bare_member_with_no_matching_definition_is_unresolved_with_an_empty_frontier() {
    let re = reproject(&unmatched_bare_graph(), UNMATCHED_CONCEPT, &Lens::Files);

    assert_eq!(
        re.total, 1,
        "the one member counts even when it resolves to nothing: {re:?}"
    );
    assert!(
        re.clusters.is_empty(),
        "an unresolvable bare member folds to NO file bucket (never mis-attributed to its referencing file): {re:?}"
    );
    assert_eq!(
        re.unresolved,
        vec![UnresolvedMember {
            id: EXTERNAL_BARE.to_string(),
            candidates: vec![],
        }],
        "a bare member matching no definition is marked unresolved with an EMPTY frontier, never dropped: {re:?}"
    );
    // The wire contract: `unresolved` is PRESENT and the empty frontier serializes as `[]`, not omitted.
    let body = serde_json::to_value(&re).expect("Reprojection serializes to JSON");
    assert_eq!(
        body["unresolved"][0]["candidates"].as_array().map(Vec::len),
        Some(0),
        "the empty frontier crosses the wire as an empty array, not an omitted key: {body}"
    );
    // The honesty exclusion: the referencing file the id encodes is NEVER a bucket.
    assert!(
        !re.clusters.iter().any(|c| c.key == "src/caller.rs"),
        "the referencing file the bare id encodes is never a bucket: {re:?}"
    );
}

const DECISION_SUBJECT: &str = "d-u55c1-a-decision-node";

/// A single NON-code subject whose id names no file (a decision node) is its own singleton member set.
/// Under the FILES lens it has no file identity, so it keeps its KIND bucket - the files-lens twin of
/// the derived-lens kind fallback - rather than being dropped or mis-attributed. Flipping to files
/// never empties a panel a code lens would fill.
#[test]
fn a_singleton_subject_with_no_file_identity_keeps_its_kind_bucket_under_files() {
    let graph = Graph {
        nodes: vec![node(DECISION_SUBJECT, KIND_DECISION, None)],
        edges: vec![],
    };

    let re = reproject(&graph, DECISION_SUBJECT, &Lens::Files);
    assert_eq!(
        re.subject, DECISION_SUBJECT,
        "the singleton echoes its subject"
    );
    assert_eq!(re.total, 1, "a single entity is a member set of one");
    assert_eq!(
        re.clusters,
        vec![Cluster {
            key: KIND_DECISION.to_string(),
            count: 1,
            kind: KIND_DECISION.to_string(),
            label: None,
        }],
        "a member with no file identity keeps its KIND bucket under files, never dropped: {re:?}"
    );
    assert!(
        re.unresolved.is_empty(),
        "a non-bare member is resolved (to its kind), so it is never marked unresolved: {re:?}"
    );
}

// --- the ROUTE COMPOSITION seam: `explain=` (the rationale overlay) precedes the c1 re-projection ----
//
// The `/api/graph` route folds SEVERAL response shapes behind one path. When the c1 re-projection arm
// (a non-empty `seed=` WITH an explicit `lens=`) was inserted, the route already carried an
// `explain=<id>[,<id>...]` rationale-overlay arm that returns FIRST and short-circuits the rest. The
// two arms COMPOSE: `explain=` is answered before the seed/lens dispatch is even reached, so a request
// that carries BOTH `explain=` and `seed=`+`lens=` is served the rationale batch, NOT a re-projection -
// neither arm supersedes the other, they are ordered. No single-function fixture guards this ordering;
// only the served route composes the two arms, so only a route-level periphery test can prove the c1
// insertion did not hijack (or get hijacked by) the overlay it sits beneath. Removing `explain=` from
// the SAME request must fall through to the re-projection, proving the composition is a true either/or
// keyed on the presence of `explain=`, not an accident of fixture shape.

const RATIONALE_DECISION: &str = "d-why-mod";

/// A `seed=`+`lens=` request that ALSO carries `explain=<seed>` is served the RATIONALE OVERLAY batch,
/// not the re-projection: the overlay arm precedes the c1 seed/lens dispatch and short-circuits it. The
/// identical request with `explain=` removed falls through to the re-projection - so the two route arms
/// compose as an either/or keyed on `explain=`, and the c1 insertion neither hijacks nor is hijacked by
/// the overlay it sits beneath.
#[test]
fn served_explain_overlay_takes_precedence_over_the_reprojection_when_both_are_present() {
    // The file-over-code fixture (whose file subject re-projects to two communities) PLUS a decision
    // that GOVERNS the file subject, so `explain=<file>` has a non-empty rationale to return.
    let mut graph = file_over_code_graph();
    graph
        .nodes
        .push(decision(RATIONALE_DECISION, "why mod.rs exists"));
    graph
        .edges
        .push(edge(RATIONALE_DECISION, FILE_SUBJECT, REL_GOVERNS));

    // BOTH `explain=` and `seed=`+`lens=` present: the overlay arm wins. The body is a RationaleBatch
    // (a `nodes` array of per-node rationale), carrying NEITHER the re-projection's `subject` nor its
    // `clusters` - so the seed/lens dispatch was never reached.
    let both = served_json(
        &graph,
        "/api/graph?explain=src%2Fpkg%2Fmod.rs&seed=src%2Fpkg%2Fmod.rs&lens=code",
    );
    assert!(
        both.get("subject").is_none() && both.get("clusters").is_none(),
        "explain= precedes the c1 re-projection: an explain+seed+lens request is NOT served a \
         re-projection (no subject / clusters key): {both}"
    );
    let nodes = both["nodes"]
        .as_array()
        .unwrap_or_else(|| panic!("the overlay batch carries a `nodes` array: {both}"));
    assert_eq!(
        nodes.len(),
        1,
        "exactly the one requested node that carries rationale appears: {both}"
    );
    assert_eq!(
        nodes[0]["node"].as_str(),
        Some(FILE_SUBJECT),
        "the overlay echoes the explained node: {both}"
    );
    let leaves = nodes[0]["leaves"]
        .as_array()
        .unwrap_or_else(|| panic!("the explained node carries its rationale leaves: {both}"));
    assert_eq!(
        leaves[0]["id"].as_str(),
        Some(RATIONALE_DECISION),
        "the leaf is the governing decision: {both}"
    );

    // The SAME request with `explain=` removed falls through to the c1 re-projection: now the body IS a
    // Reprojection (subject + clusters), NOT the overlay - proving the composition is keyed on the
    // PRESENCE of `explain=`, and that removing it restores the re-projection arm unchanged.
    let reproj = served_json(&graph, "/api/graph?seed=src%2Fpkg%2Fmod.rs&lens=code");
    assert_eq!(
        reproj["subject"].as_str(),
        Some(FILE_SUBJECT),
        "with explain= gone, the same seed+lens re-projects the subject: {reproj}"
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
        "the re-projection buckets the file's contained entities by community: {reproj}"
    );
    assert!(
        reproj.get("nodes").is_none(),
        "the re-projection is NOT the overlay batch (no nodes array): {reproj}"
    );
}
