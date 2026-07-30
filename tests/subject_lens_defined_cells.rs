//! Periphery (API / integration) tests for spec 55 CRITERION 2: the DEFINED CELLS of the subject x
//! lens matrix. Criterion 1 owns the re-projection MECHANICS (member set -> re-bucket under a lens)
//! and its file-resolution honesty; this criterion owns the MATRIX COMPLETENESS - every (subject,
//! lens) cell, including the empty and degenerate ones, is defined, never an error and never an
//! invention:
//!
//!  - THE EMPTY CELL: a subject whose members carry NO membership under a DERIVED lens returns the
//!    documented empty-cell state - `no derived communities` under [`Lens::Code`], `not part of any
//!    concept` under [`Lens::Concepts`] - so the panel captions the absence rather than showing a
//!    bare kind-bucket view with no explanation. The kind-fallback clusters criterion 1 ships still
//!    render (nothing is dropped); the message is ADDITIVE. A [`Lens::Files`] re-grain always resolves,
//!    so it never carries the message.
//!  - THE WIDE CELL: a re-grain whose bucket count EXCEEDS the render budget truncates through the
//!    SAME [`CLUSTER_RENDER_BUDGET`] the cluster drill uses - the largest buckets are kept (ties by
//!    key), the cross-bucket edges are pruned to the kept set so none dangles, and `truncated` carries
//!    the FULL bucket count for the panel's "showing N of M" caption. An at/under-budget re-grain omits
//!    it, so a small cell stays lean.
//!  - THE SHARED MEMBER: a member realizing MORE THAN ONE concept appears ONCE (its primary bucket,
//!    per criterion 1) and is FLAGGED in `shared` - the re-projection's twin of the drill's per-node
//!    `shared` marker - so a multi-bucket member is never silently duplicated nor silently hidden.
//!    Empty under [`Lens::Code`] / [`Lens::Files`] (a node carries at most one community, and files
//!    never share).
//!
//! These run OUTSIDE the crate over the library's PUBLIC surface (`rigger::dash::{reproject,
//! Reprojection, Cluster, ClusterEdge, Lens, CLUSTER_RENDER_BUDGET, REPROJECT_NO_COMMUNITY,
//! REPROJECT_NO_CONCEPT}`), so they guard the exact public boundary the served panel consumes, and
//! drive the served `route` end-to-end.

use std::collections::HashMap;

use rigger::contextgraph::{
    Edge, Graph, Node, KIND_CODE_ENTITY, KIND_COMMUNITY, KIND_CONCEPT, KIND_FILE, REL_CALLS,
    REL_CONTAINS, REL_IN_COMMUNITY, REL_REALIZES,
};
use rigger::dash::{
    reproject, route, Cluster, ClusterEdge, Lens, CLUSTER_RENDER_BUDGET, REPROJECT_NO_COMMUNITY,
    REPROJECT_NO_CONCEPT,
};

// --- fixture helpers ----------------------------------------------------------------------------

/// A code-entity DEFINITION node (a `name` attr marks it a real definition, so a files re-grain folds
/// it under its OWN file and a derived lens reads its memberships).
fn def(id: &str, name: &str) -> Node {
    let mut n = Node {
        id: id.to_string(),
        kind: KIND_CODE_ENTITY.to_string(),
        attrs: Default::default(),
    };
    n.attrs.insert("name".to_string(), name.to_string());
    n
}

/// A plain node of a given kind (a `file` subject, or a `community` / `concept` super-node carrying
/// its deterministic display `label`).
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

fn code_lens() -> Lens {
    Lens::from_query(Some("code"), Some("1"))
}

fn concepts_lens() -> Lens {
    Lens::from_query(Some("concepts"), Some("1"))
}

/// A code-entity kind-fallback bucket of `count` members (the membership-less fold criterion 1 ships).
fn kind_bucket(count: usize) -> Cluster {
    Cluster {
        key: KIND_CODE_ENTITY.to_string(),
        count,
        kind: KIND_CODE_ENTITY.to_string(),
        label: None,
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
    assert_eq!(resp.status, 200, "GET {target} must be served 200");
    serde_json::from_slice(&resp.body)
        .unwrap_or_else(|e| panic!("the served {target} body must be valid JSON: {e}"))
}

// --- THE EMPTY CELL: a derived lens with no membership carries the documented message ---------------

const FILE_SUBJECT: &str = "src/pkg/mod.rs";
const COMM_X: &str = "community/1/0";
const COMM_Y: &str = "community/1/1";

/// A file whose two contained entities are in COMMUNITIES but realize NO concept. So under the code
/// lens the cell is FULL (two community buckets), while under the concepts lens the cell is EMPTY (the
/// members are part of no concept) - proving the empty-cell state is per-lens membership, not the
/// subject.
fn communities_no_concepts_graph() -> Graph {
    Graph {
        nodes: vec![
            node(FILE_SUBJECT, KIND_FILE, None),
            node(COMM_X, KIND_COMMUNITY, Some("x")),
            node(COMM_Y, KIND_COMMUNITY, Some("y")),
            def("src/pkg/mod.rs::e1", "e1"),
            def("src/pkg/mod.rs::e2", "e2"),
        ],
        edges: vec![
            edge(FILE_SUBJECT, "src/pkg/mod.rs::e1", REL_CONTAINS),
            edge(FILE_SUBJECT, "src/pkg/mod.rs::e2", REL_CONTAINS),
            edge("src/pkg/mod.rs::e1", COMM_X, REL_IN_COMMUNITY),
            edge("src/pkg/mod.rs::e2", COMM_Y, REL_IN_COMMUNITY),
        ],
    }
}

/// A derived lens whose re-grain folds NO member into a community/concept bucket returns the documented
/// empty-cell MESSAGE (per lens, never swapped), WITH the criterion-1 kind-fallback clusters still
/// rendered - the message is additive, not a replacement. The full cell (same subject, code lens) never
/// carries it.
#[test]
fn a_derived_regrain_with_no_membership_carries_the_documented_empty_cell_message() {
    let graph = communities_no_concepts_graph();

    // CONCEPTS lens: the two entities realize no concept -> the empty cell.
    let concepts = reproject(&graph, FILE_SUBJECT, &concepts_lens());
    assert_eq!(
        concepts.empty_state.as_deref(),
        Some(REPROJECT_NO_CONCEPT),
        "a concepts re-grain with no realized concept carries the `not part of any concept` message: {concepts:?}"
    );
    assert_eq!(
        concepts.clusters,
        vec![kind_bucket(2)],
        "the kind-fallback clusters criterion 1 ships still render - the message is ADDITIVE: {concepts:?}"
    );
    assert_eq!(
        concepts.total, 2,
        "the member-set size is unchanged by the message"
    );

    // CODE lens: the two entities ARE in communities -> a FULL cell, no message.
    let code = reproject(&graph, FILE_SUBJECT, &code_lens());
    assert_eq!(
        code.empty_state, None,
        "a re-grain that DOES fold into community buckets carries no empty-cell message: {code:?}"
    );
    let keys: Vec<&str> = code.clusters.iter().map(|c| c.key.as_str()).collect();
    assert_eq!(
        keys,
        vec![COMM_X, COMM_Y],
        "the full code cell buckets by community, no message: {code:?}"
    );
}

const CONCEPT_ONLY: &str = "concept/5/0";

/// The code-lens twin: a concept subject whose one member realizes the concept but is in NO community
/// returns `no derived communities` under the code lens, while the FILES lens - which always resolves -
/// never carries any empty-cell message even for an unknown (memberless) subject.
#[test]
fn the_code_lens_empty_cell_and_the_files_lens_never_carrying_a_message() {
    let graph = Graph {
        nodes: vec![
            node(CONCEPT_ONLY, KIND_CONCEPT, Some("idea")),
            def("src/a.rs::only", "only"),
        ],
        edges: vec![edge("src/a.rs::only", CONCEPT_ONLY, REL_REALIZES)],
    };

    // CODE lens: the member is in no community -> the code empty cell.
    let code = reproject(&graph, CONCEPT_ONLY, &code_lens());
    assert_eq!(
        code.empty_state.as_deref(),
        Some(REPROJECT_NO_COMMUNITY),
        "a code re-grain with no community membership carries the `no derived communities` message: {code:?}"
    );
    assert_eq!(
        code.clusters,
        vec![kind_bucket(1)],
        "the code empty cell still renders the member's kind-fallback bucket: {code:?}"
    );

    // FILES lens: a file re-grain always resolves, so it NEVER carries an empty-cell message.
    let files = reproject(&graph, CONCEPT_ONLY, &Lens::Files);
    assert_eq!(
        files.empty_state, None,
        "a files re-grain resolves every member, so it carries no empty-cell message: {files:?}"
    );
    // An UNKNOWN subject (empty member set) under files is a degenerate-but-defined cell: still no
    // message (the honesty rules never invent a lens membership a files view does not have).
    let unknown = reproject(&graph, "no/such/subject", &Lens::Files);
    assert_eq!(
        unknown.total, 0,
        "an unknown subject has an empty member set"
    );
    assert_eq!(
        unknown.empty_state, None,
        "the files lens never carries an empty-cell message, even for an unknown subject: {unknown:?}"
    );
}

// --- THE PARTIAL (MIXED-MEMBERSHIP) CELL: at least one, but not every, member folds derived ----------

const MIXED_FILE: &str = "src/mix/mod.rs";
const MIXED_COMM: &str = "community/1/0";
const MIXED_CONCEPT: &str = "concept/1/0";

/// A file subject with two contained entities where EXACTLY ONE folds into a derived bucket: `e1` is
/// both in a community AND realizes a concept, while `e2` carries no membership at all. So under EITHER
/// derived lens the re-grain is PARTIALLY membered (one derived bucket beside one kind-fallback
/// bucket) - the boundary no other empty-cell fixture reaches (every other one is all-membered or
/// none-membered, where the membership test cannot tell "any member" from "every member" apart).
fn mixed_membership_graph() -> Graph {
    Graph {
        nodes: vec![
            node(MIXED_FILE, KIND_FILE, None),
            node(MIXED_COMM, KIND_COMMUNITY, Some("cx")),
            node(MIXED_CONCEPT, KIND_CONCEPT, Some("cc")),
            def("src/mix/mod.rs::e1", "e1"),
            def("src/mix/mod.rs::e2", "e2"),
        ],
        edges: vec![
            edge(MIXED_FILE, "src/mix/mod.rs::e1", REL_CONTAINS),
            edge(MIXED_FILE, "src/mix/mod.rs::e2", REL_CONTAINS),
            // e1 folds into a derived bucket under BOTH lenses; e2 folds into neither.
            edge("src/mix/mod.rs::e1", MIXED_COMM, REL_IN_COMMUNITY),
            edge("src/mix/mod.rs::e1", MIXED_CONCEPT, REL_REALIZES),
        ],
    }
}

/// A PARTIALLY-membered derived cell is FULL, never empty: when at least one - but not every - member
/// folds into a community/concept bucket, `empty_state` stays `None` (the documented boundary "a single
/// member with a derived membership makes the cell full and clears the message"). The kind-fallback
/// bucket for the membership-less member still renders BESIDE the derived bucket, so the cell is
/// genuinely mixed. Holds under BOTH derived lenses. This is the boundary the all-membered and
/// none-membered fixtures cannot reach: it distinguishes the "ANY member folds" rule (full) from an
/// "EVERY member folds" one, which would wrongly caption a partially-membered cell as empty.
#[test]
fn a_partially_membered_derived_cell_is_full_under_both_derived_lenses() {
    let graph = mixed_membership_graph();

    // CODE lens: e1 is in a community, e2 is not -> a MIXED cell, no empty-cell message.
    let code = reproject(&graph, MIXED_FILE, &code_lens());
    assert_eq!(
        code.total, 2,
        "the member set is {{e1, e2}} regardless of membership: {code:?}"
    );
    assert_eq!(
        code.empty_state, None,
        "one member folding into a community bucket makes the cell FULL - no empty-cell message: {code:?}"
    );
    assert_eq!(
        code.clusters,
        vec![
            // e2 keeps its kind-fallback bucket ("code-entity" sorts before "community/..").
            kind_bucket(1),
            Cluster {
                key: MIXED_COMM.to_string(),
                count: 1,
                kind: KIND_CODE_ENTITY.to_string(),
                label: Some("cx".to_string()),
            },
        ],
        "the mixed cell renders BOTH the derived bucket and the kind-fallback bucket: {code:?}"
    );

    // CONCEPTS lens: e1 realizes a concept, e2 does not -> equally a MIXED, full cell.
    let concepts = reproject(&graph, MIXED_FILE, &concepts_lens());
    assert_eq!(
        concepts.total, 2,
        "the member set is {{e1, e2}} under concepts too: {concepts:?}"
    );
    assert_eq!(
        concepts.empty_state, None,
        "one member realizing a concept makes the cell FULL - no empty-cell message: {concepts:?}"
    );
    assert_eq!(
        concepts.clusters,
        vec![
            kind_bucket(1),
            Cluster {
                key: MIXED_CONCEPT.to_string(),
                count: 1,
                kind: KIND_CODE_ENTITY.to_string(),
                label: Some("cc".to_string()),
            },
        ],
        "the mixed concepts cell renders the concept bucket beside the kind-fallback bucket: {concepts:?}"
    );
    // e1 realizes exactly one concept, so it is NOT flagged shared - a mixed cell is not a shared one.
    assert!(
        concepts.shared.is_empty(),
        "a single-concept member is never flagged shared: {concepts:?}"
    );
}

// --- THE ABSENT-SUBJECT DERIVED CELL: an empty member set is the documented empty cell --------------

/// The (absent subject x DERIVED lens) defined cell: a subject NOT in the graph has an empty member set,
/// so no member folds into a derived bucket and the re-grain is the documented empty cell under BOTH
/// derived lenses - `no derived communities` under code, `not part of any concept` under concepts -
/// with zero clusters over a zero-size member set. This pins the derived arms of the absent-subject row
/// the Files twin (`the_code_lens_empty_cell_and_the_files_lens_never_carrying_a_message`) leaves open:
/// there the absent subject is asserted only under Files (which never carries a message), so without
/// this the two derived arms could regress in either direction undetected.
#[test]
fn an_absent_subject_under_a_derived_lens_is_the_documented_empty_cell() {
    // Any graph works; the subject is simply not one of its nodes.
    let graph = communities_no_concepts_graph();

    let code = reproject(&graph, "no/such/subject", &code_lens());
    assert_eq!(
        code.total, 0,
        "an absent subject has an empty member set: {code:?}"
    );
    assert!(
        code.clusters.is_empty(),
        "an absent subject folds into no buckets: {code:?}"
    );
    assert_eq!(
        code.empty_state.as_deref(),
        Some(REPROJECT_NO_COMMUNITY),
        "the absent-subject code cell carries the documented empty-cell message: {code:?}"
    );

    let concepts = reproject(&graph, "no/such/subject", &concepts_lens());
    assert_eq!(
        concepts.total, 0,
        "an absent subject has an empty member set under concepts: {concepts:?}"
    );
    assert!(
        concepts.clusters.is_empty(),
        "an absent subject folds into no buckets under concepts: {concepts:?}"
    );
    assert_eq!(
        concepts.empty_state.as_deref(),
        Some(REPROJECT_NO_CONCEPT),
        "the absent-subject concepts cell carries the documented empty-cell message: {concepts:?}"
    );
}

// --- THE WIDE CELL: a re-grain over budget truncates with the count caption -------------------------

const WIDE_CONCEPT: &str = "concept/2/0";

/// A concept realizing `CLUSTER_RENDER_BUDGET + 1` definition members, each in its OWN file (so a files
/// re-grain yields one bucket per file), plus two cross-file coupling edges: one INTO the dropped
/// bucket and one wholly within the kept set.
fn wide_files_graph() -> Graph {
    let n = CLUSTER_RENDER_BUDGET + 1;
    let mut nodes = vec![node(WIDE_CONCEPT, KIND_CONCEPT, Some("wide"))];
    let mut edges = Vec::new();
    for i in 0..n {
        // Zero-padded so the file KEY ordering is lexicographic 00 < 01 < ... < 60: the largest key
        // (index n-1) is the one the (count-tie, key-asc) rank drops.
        let id = format!("src/f{i:02}.rs::e{i}");
        nodes.push(def(&id, &format!("e{i}")));
        edges.push(edge(&id, WIDE_CONCEPT, REL_REALIZES));
    }
    // A coupling edge from the LAST (dropped) member into the first: its super-edge must be pruned.
    edges.push(edge(
        &format!("src/f{:02}.rs::e{}", n - 1, n - 1),
        "src/f00.rs::e0",
        REL_CALLS,
    ));
    // A coupling edge wholly within the kept set (f00 -> f01): its super-edge must survive.
    edges.push(edge("src/f00.rs::e0", "src/f01.rs::e1", REL_CALLS));
    Graph { nodes, edges }
}

/// A re-grain whose bucket count exceeds [`CLUSTER_RENDER_BUDGET`] keeps exactly the budget's worth of
/// (largest, ties by key) buckets, reports the FULL bucket count in `truncated` for the "showing N of
/// M" caption, and PRUNES every cross-bucket edge touching a dropped bucket so none dangles. An
/// at/under-budget re-grain omits `truncated` entirely.
#[test]
fn a_wide_regrain_caps_to_the_render_budget_and_prunes_dangling_edges() {
    let re = reproject(&wide_files_graph(), WIDE_CONCEPT, &Lens::Files);

    let m = CLUSTER_RENDER_BUDGET + 1;
    assert_eq!(
        re.total, m,
        "total is the full member-set size, never truncated"
    );
    assert_eq!(
        re.clusters.len(),
        CLUSTER_RENDER_BUDGET,
        "the rendered bucket count is capped to the render budget: {}",
        re.clusters.len()
    );
    assert_eq!(
        re.truncated,
        Some(m),
        "truncated carries the FULL bucket count (M) for the showing-N-of-M caption: {re:?}"
    );
    // The kept buckets are the 60 lexicographically-smallest file keys (f00..f59); the largest (f60)
    // is dropped - a deterministic cap.
    let dropped = format!("src/f{:02}.rs", m - 1);
    let keys: Vec<&str> = re.clusters.iter().map(|c| c.key.as_str()).collect();
    assert!(
        !keys.contains(&dropped.as_str()),
        "the largest-key bucket is the one the cap drops: {re:?}"
    );
    assert_eq!(keys[0], "src/f00.rs", "the kept buckets stay sorted by key");
    // Edge pruning: the survivor (f00 -> f01) rides; the edge into the dropped f60 bucket is gone.
    assert!(
        re.edges.contains(&ClusterEdge {
            from: "src/f00.rs".to_string(),
            to: "src/f01.rs".to_string(),
            weight: 1,
        }),
        "a cross-bucket edge wholly within the kept set survives the cap: {re:?}"
    );
    assert!(
        !re.edges
            .iter()
            .any(|e| e.from == dropped || e.to == dropped),
        "an edge touching a dropped bucket is pruned so none dangles: {re:?}"
    );

    // The wire contract: `truncated` present when the cap fired, ABSENT for an under-budget re-grain.
    let body = serde_json::to_value(&re).unwrap();
    assert!(
        body.as_object().unwrap().contains_key("truncated"),
        "a capped body carries `truncated`: {body}"
    );

    // An at/under-budget re-grain omits `truncated` (a lean small cell).
    let small = reproject(
        &Graph {
            nodes: vec![
                node(WIDE_CONCEPT, KIND_CONCEPT, Some("wide")),
                def("src/one.rs::a", "a"),
            ],
            edges: vec![edge("src/one.rs::a", WIDE_CONCEPT, REL_REALIZES)],
        },
        WIDE_CONCEPT,
        &Lens::Files,
    );
    assert_eq!(
        small.truncated, None,
        "an under-budget re-grain is not truncated"
    );
    assert!(
        !serde_json::to_value(&small)
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("truncated"),
        "an under-budget body omits `truncated`"
    );
}

// --- THE SHARED MEMBER: a multi-concept member appears once, flagged ---------------------------------

const SUB_C: &str = "concept/1/0";
const OTHER_D: &str = "concept/1/1";
const SHARED_MEMBER: &str = "src/a.rs::m";
const SOLO_MEMBER: &str = "src/b.rs::n";

/// The concept subject `C` has two members: `n` realizes only `C`, and `m` realizes BOTH `C` and `D`.
/// `D` is realized by two more nodes (so `D` is the larger concept and is `m`'s PRIMARY bucket). Under
/// the concepts re-grain of `C`'s members, `m` appears ONCE (in `D`) and is flagged `shared`.
fn shared_member_graph() -> Graph {
    Graph {
        nodes: vec![
            node(SUB_C, KIND_CONCEPT, Some("cc")),
            node(OTHER_D, KIND_CONCEPT, Some("dd")),
            def(SHARED_MEMBER, "m"),
            def(SOLO_MEMBER, "n"),
            def("src/c.rs::p", "p"),
            def("src/d.rs::q", "q"),
        ],
        edges: vec![
            // C's members: m and n.
            edge(SHARED_MEMBER, SUB_C, REL_REALIZES),
            edge(SOLO_MEMBER, SUB_C, REL_REALIZES),
            // m ALSO realizes D; D has two more realizers (p, q), so D is the larger -> m's primary.
            edge(SHARED_MEMBER, OTHER_D, REL_REALIZES),
            edge("src/c.rs::p", OTHER_D, REL_REALIZES),
            edge("src/d.rs::q", OTHER_D, REL_REALIZES),
        ],
    }
}

/// A member realizing more than one concept appears ONCE (its primary bucket) and is FLAGGED in
/// `shared`; a member realizing exactly one is never flagged. The bucket counts sum to the member-set
/// size (no silent duplication), and the flag is empty under the code and files lenses.
#[test]
fn a_multi_concept_member_appears_once_and_is_flagged_shared() {
    let graph = shared_member_graph();
    let re = reproject(&graph, SUB_C, &concepts_lens());

    assert_eq!(re.total, 2, "C's member set is {{m, n}}");
    assert_eq!(
        re.shared,
        vec![SHARED_MEMBER.to_string()],
        "the multi-concept member m is flagged shared; the single-concept n is not: {re:?}"
    );
    // m folds under its PRIMARY (D, the larger concept), n under C - each appears exactly once, so the
    // bucket counts sum to the member-set size.
    assert_eq!(
        re.clusters,
        vec![
            Cluster {
                key: SUB_C.to_string(),
                count: 1,
                kind: KIND_CODE_ENTITY.to_string(),
                label: Some("cc".to_string()),
            },
            Cluster {
                key: OTHER_D.to_string(),
                count: 1,
                kind: KIND_CODE_ENTITY.to_string(),
                label: Some("dd".to_string()),
            },
        ],
        "the shared member appears once (in its primary bucket D), never duplicated: {re:?}"
    );
    assert_eq!(
        re.clusters.iter().map(|c| c.count).sum::<usize>(),
        re.total,
        "every member appears exactly once across the buckets"
    );

    // The wire contract: `shared` present when a member is flagged, absent otherwise.
    let body = serde_json::to_value(&re).unwrap();
    assert_eq!(
        body["shared"].as_array().map(Vec::len),
        Some(1),
        "the shared member id crosses the wire: {body}"
    );

    // A node carries at most ONE community and files never share, so `shared` is empty under those.
    let code = reproject(&graph, SUB_C, &code_lens());
    assert!(
        code.shared.is_empty(),
        "the code lens never flags a shared member: {code:?}"
    );
    let files = reproject(&graph, SUB_C, &Lens::Files);
    assert!(
        files.shared.is_empty(),
        "the files lens never flags a shared member: {files:?}"
    );
    // The empty-cell fixture (no shared member) omits `shared` from its JSON body.
    assert!(
        !serde_json::to_value(reproject(
            &communities_no_concepts_graph(),
            FILE_SUBJECT,
            &code_lens()
        ))
        .unwrap()
        .as_object()
        .unwrap()
        .contains_key("shared"),
        "a re-grain with no shared member omits `shared` from the body"
    );
}

// --- THE SERVED ROUTE: the empty-cell message rides the served body ---------------------------------

/// The `seed=`+`lens=` route serves the empty-cell message end-to-end: a served concepts re-grain of a
/// subject whose members realize no concept carries `empty_state` in the JSON body, so the client can
/// caption the defined-but-empty cell without a second request.
#[test]
fn the_served_route_carries_the_empty_cell_message() {
    let graph = communities_no_concepts_graph();
    let body = served_json(
        &graph,
        "/api/graph?seed=src%2Fpkg%2Fmod.rs&lens=concepts&resolution=1",
    );
    assert_eq!(
        body["subject"].as_str(),
        Some(FILE_SUBJECT),
        "the served re-projection echoes its subject: {body}"
    );
    assert_eq!(
        body["empty_state"].as_str(),
        Some(REPROJECT_NO_CONCEPT),
        "the served empty-cell re-grain carries its documented message: {body}"
    );
}
