//! SDET periphery CONTRACT layer for spec 55 criterion 2 (the defined cells of the subject x lens
//! re-projection). It is the OUTSIDE-IN complement to the mechanics coverage: it pins the BOUNDARY
//! values, the wire CONTRACT, and the served-route INTEGRATION seam that the mechanics tests leave
//! open, all over the library's PUBLIC surface (`rigger::dash::{reproject, route, Reprojection,
//! Cluster, ClusterEdge, Lens, CLUSTER_RENDER_BUDGET, REPROJECT_NO_COMMUNITY, REPROJECT_NO_CONCEPT}`).
//!
//! Four boundaries the panel depends on, none of them exercised by the mechanics fixtures:
//!
//!  - THE AT-BUDGET EDGE: a re-grain with EXACTLY [`CLUSTER_RENDER_BUDGET`] buckets is NOT capped -
//!    `truncated` stays `None` and nothing is pruned. Guards the `<=` off-by-one (a `<` bound would
//!    wrongly truncate a cell that fits exactly).
//!  - THE LARGEST-BY-COUNT RANK: an over-budget cap keeps the LARGEST buckets, ties by key ascending.
//!    A fixture whose buckets are all the same size can never tell a count-primary rank from a pure
//!    key-ascending one; this drives DIFFERENT sizes, so a large bucket with a HIGH key is kept while
//!    a small bucket with a LOWER key is dropped - the only shape that distinguishes the two.
//!  - THE SERVED SEAM: the `/api/graph?seed=&lens=` route carries `shared` and `truncated` end-to-end,
//!    so the client reads the flag and the "showing N of M" count with no second request.
//!  - THE WIRE BACK-COMPAT: when the three criterion-2 fields are at their default (empty / `None`),
//!    the serialized body carries NONE of their keys - byte-identical to the pre-criterion-2 shape a
//!    files or full-derived cell already emitted - and the body is deterministic across re-projections.

use std::collections::HashMap;

use rigger::contextgraph::{
    Edge, Graph, Node, KIND_CODE_ENTITY, KIND_COMMUNITY, KIND_CONCEPT, REL_CALLS, REL_IN_COMMUNITY,
    REL_REALIZES,
};
use rigger::dash::{
    reproject, route, ClusterEdge, Lens, CLUSTER_RENDER_BUDGET, REPROJECT_NO_COMMUNITY,
    REPROJECT_NO_CONCEPT,
};

// --- fixture helpers ----------------------------------------------------------------------------

/// A code-entity DEFINITION node (its `name` attr marks it a real definition, so a files re-grain
/// folds it under its OWN file and a derived lens reads its memberships).
fn def(id: &str, name: &str) -> Node {
    let mut n = Node {
        id: id.to_string(),
        kind: KIND_CODE_ENTITY.to_string(),
        attrs: Default::default(),
    };
    n.attrs.insert("name".to_string(), name.to_string());
    n
}

/// A plain node of a given kind (a `community` / `concept` super-node carrying its display `label`).
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

/// The bucket keys a re-projection rendered, in order.
fn keys(re: &rigger::dash::Reprojection) -> Vec<&str> {
    re.clusters.iter().map(|c| c.key.as_str()).collect()
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

const WIDE_CONCEPT: &str = "concept/2/0";

/// A concept realizing `single` members each in its OWN file (`src/f00.rs`..), so a files re-grain
/// yields exactly one bucket per file. When `big` > 0, an extra file `src/zz.rs` holds `big` members
/// (one bucket of size `big`, whose key sorts AFTER every `src/fNN.rs`). A coupling edge `f00 -> f01`
/// is always added so a within-kept-set super-edge is present to assert against.
fn per_file_graph(single: usize, big: usize) -> Graph {
    let mut nodes = vec![node(WIDE_CONCEPT, KIND_CONCEPT, Some("wide"))];
    let mut edges = Vec::new();
    for i in 0..single {
        // Zero-padded so the file KEY order is lexicographic 00 < 01 < ... : the largest fNN key is
        // the one a (count-tie, key-asc) rank drops first.
        let id = format!("src/f{i:02}.rs::e{i}");
        nodes.push(def(&id, &format!("e{i}")));
        edges.push(edge(&id, WIDE_CONCEPT, REL_REALIZES));
    }
    for j in 0..big {
        let id = format!("src/zz.rs::b{j}");
        nodes.push(def(&id, &format!("b{j}")));
        edges.push(edge(&id, WIDE_CONCEPT, REL_REALIZES));
    }
    if single >= 2 {
        // A cross-file coupling edge wholly within the smallest-key buckets, so it is always kept.
        edges.push(edge("src/f00.rs::e0", "src/f01.rs::e1", REL_CALLS));
    }
    Graph { nodes, edges }
}

// --- THE AT-BUDGET EDGE -------------------------------------------------------------------------

/// A re-grain whose bucket count is EXACTLY [`CLUSTER_RENDER_BUDGET`] is not a wide cell: every bucket
/// is kept, no edge is pruned, and `truncated` is `None` / omitted from the wire. This is the `<=`
/// boundary the mechanics fixtures skip (they drive budget + 1 and a singleton, never the exact edge),
/// where a `<` off-by-one would truncate a cell that fits.
#[test]
fn a_re_grain_of_exactly_the_render_budget_is_not_truncated_and_prunes_nothing() {
    let graph = per_file_graph(CLUSTER_RENDER_BUDGET, 0);
    let re = reproject(&graph, WIDE_CONCEPT, &Lens::Files);

    assert_eq!(
        re.clusters.len(),
        CLUSTER_RENDER_BUDGET,
        "a subject with exactly the budget's worth of file buckets renders every one"
    );
    assert_eq!(
        re.truncated, None,
        "an AT-budget re-grain is not a wide cell - `truncated` stays None (guards the <= edge): {re:?}"
    );
    assert!(
        re.edges.contains(&ClusterEdge {
            from: "src/f00.rs".to_string(),
            to: "src/f01.rs".to_string(),
            weight: 1,
        }),
        "at budget nothing is dropped, so a cross-bucket edge is never pruned: {re:?}"
    );

    let body = serde_json::to_value(&re).unwrap();
    assert!(
        !body.as_object().unwrap().contains_key("truncated"),
        "an at-budget body omits `truncated` from the wire: {body}"
    );
}

// --- THE LARGEST-BY-COUNT RANK ------------------------------------------------------------------

/// The cap keeps the LARGEST buckets, ties by key ascending - not merely the lexicographically-first.
/// With `CLUSTER_RENDER_BUDGET` single-member file buckets PLUS one `src/zz.rs` bucket of three
/// members (its key sorts LAST), the over-budget cap must KEEP the large high-key `zz` bucket and DROP
/// the largest-key SINGLE-member bucket (`src/fNN.rs`). A pure key-ascending cap would do the opposite
/// (drop `zz`), so this is the fixture shape that distinguishes count-primary ranking from key-only -
/// the mechanics fixture's uniform count-1 buckets cannot.
#[test]
fn an_over_budget_cap_keeps_the_largest_bucket_by_count_not_the_lowest_key() {
    let graph = per_file_graph(CLUSTER_RENDER_BUDGET, 3);
    let re = reproject(&graph, WIDE_CONCEPT, &Lens::Files);

    let bucket_count = CLUSTER_RENDER_BUDGET + 1; // 60 single-member files + the one `zz` bucket
    let member_count = CLUSTER_RENDER_BUDGET + 3; // 60 single members + 3 in `zz`
    assert_eq!(
        re.total, member_count,
        "total is the full member-set size, untouched by the bucket cap"
    );
    assert_eq!(
        re.clusters.len(),
        CLUSTER_RENDER_BUDGET,
        "the rendered bucket count is capped to the render budget"
    );
    assert_eq!(
        re.truncated,
        Some(bucket_count),
        "`truncated` carries the FULL bucket count M for the showing-N-of-M caption: {re:?}"
    );

    let kept = keys(&re);
    let zz = re
        .clusters
        .iter()
        .find(|c| c.key == "src/zz.rs")
        .unwrap_or_else(|| panic!("the large high-key bucket must survive the cap: {re:?}"));
    assert_eq!(
        zz.count, 3,
        "the `zz` bucket folded all three of its members: {re:?}"
    );
    assert!(
        kept.contains(&"src/zz.rs"),
        "the LARGEST bucket is kept even though its key sorts last (count beats key): {re:?}"
    );
    let dropped = format!("src/f{:02}.rs", CLUSTER_RENDER_BUDGET - 1);
    assert!(
        !kept.contains(&dropped.as_str()),
        "the dropped bucket is the largest-key SINGLE-member one ({dropped}), the correct tie-break: {re:?}"
    );
    // Every kept single-member key is strictly smaller than the dropped one - the deterministic pick.
    assert!(
        kept.iter()
            .filter(|k| k.starts_with("src/f"))
            .all(|k| *k < dropped.as_str()),
        "the kept single-member buckets are the lexicographically-smallest ones: {re:?}"
    );
}

// --- THE SERVED SEAM: shared + truncated ride the route -----------------------------------------

const SUB_C: &str = "concept/1/0";
const OTHER_D: &str = "concept/1/1";
const SHARED_MEMBER: &str = "src/a.rs::m";
const SOLO_MEMBER: &str = "src/b.rs::n";

/// Concept `C` has members `{m, n}`; `m` ALSO realizes `D`, and `D` (three realizers) is the larger
/// concept, so it is `m`'s PRIMARY. A concepts re-grain of `C` folds `m` under `D` once and flags it
/// `shared`.
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
            edge(SHARED_MEMBER, SUB_C, REL_REALIZES),
            edge(SOLO_MEMBER, SUB_C, REL_REALIZES),
            edge(SHARED_MEMBER, OTHER_D, REL_REALIZES),
            edge("src/c.rs::p", OTHER_D, REL_REALIZES),
            edge("src/d.rs::q", OTHER_D, REL_REALIZES),
        ],
    }
}

/// The served `/api/graph?seed=&lens=concepts` route carries the `shared` flag end-to-end, so the
/// panel marks the multi-concept member without a second request. The mechanics served test only
/// drives the empty-cell message; the shared flag has no served coverage without this.
#[test]
fn the_served_route_carries_the_shared_member_flag() {
    let graph = shared_member_graph();
    // `concept/1/0` -> percent-encoded slashes, exactly as the client's encodeURIComponent emits.
    let body = served_json(
        &graph,
        "/api/graph?seed=concept%2F1%2F0&lens=concepts&resolution=1",
    );
    assert_eq!(
        body["subject"].as_str(),
        Some(SUB_C),
        "the served re-projection echoes its subject: {body}"
    );
    let shared: Vec<&str> = body["shared"]
        .as_array()
        .unwrap_or_else(|| panic!("the served body must carry a `shared` array: {body}"))
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        shared,
        vec![SHARED_MEMBER],
        "the multi-concept member crosses the wire flagged shared: {body}"
    );
}

/// The served `/api/graph?seed=&lens=files` route carries the wide-cell `truncated` count and the
/// capped bucket list end-to-end, so the client can caption "showing N of M" without re-deriving it.
/// The mechanics served test never drives an over-budget subject.
#[test]
fn the_served_route_carries_the_wide_cell_truncation() {
    let graph = per_file_graph(CLUSTER_RENDER_BUDGET + 1, 0);
    let body = served_json(
        &graph,
        "/api/graph?seed=concept%2F2%2F0&lens=files&resolution=1",
    );
    assert_eq!(
        body["truncated"].as_u64(),
        Some((CLUSTER_RENDER_BUDGET + 1) as u64),
        "the served wide cell carries the full bucket count M for the caption: {body}"
    );
    assert_eq!(
        body["clusters"].as_array().map(Vec::len),
        Some(CLUSTER_RENDER_BUDGET),
        "the served wide cell renders exactly the budget's worth of buckets: {body}"
    );
}

// --- THE WIRE BACK-COMPAT + DETERMINISM ---------------------------------------------------------

const PLAIN_CONCEPT: &str = "concept/3/0";
const COMM_A: &str = "community/1/0";
const COMM_B: &str = "community/1/1";

/// A concept whose two members sit in two files and two communities: under FILES a fully-resolved
/// two-bucket cell, under CODE a two-community cell - each a FULL, in-budget, non-shared re-grain, so
/// all three criterion-2 fields (`shared`, `truncated`, `empty_state`) are at their default.
fn full_in_budget_graph() -> Graph {
    Graph {
        nodes: vec![
            node(PLAIN_CONCEPT, KIND_CONCEPT, Some("plain")),
            node(COMM_A, KIND_COMMUNITY, Some("a")),
            node(COMM_B, KIND_COMMUNITY, Some("b")),
            def("src/one.rs::x", "x"),
            def("src/two.rs::y", "y"),
        ],
        edges: vec![
            edge("src/one.rs::x", PLAIN_CONCEPT, REL_REALIZES),
            edge("src/two.rs::y", PLAIN_CONCEPT, REL_REALIZES),
            edge("src/one.rs::x", COMM_A, REL_IN_COMMUNITY),
            edge("src/two.rs::y", COMM_B, REL_IN_COMMUNITY),
        ],
    }
}

/// When the three criterion-2 fields are at their default, the serialized body carries NONE of their
/// keys, so a pre-criterion-2 consumer sees the byte-identical shape a plain files or full-derived
/// cell already emitted. Holds under BOTH a files re-grain (fields never apply) and a full code
/// re-grain (they apply but are cleared). This is the additive-not-breaking wire contract.
#[test]
fn a_full_in_budget_re_grain_omits_every_defaulted_criterion_2_field() {
    let graph = full_in_budget_graph();

    for lens in [Lens::Files, code_lens()] {
        let re = reproject(&graph, PLAIN_CONCEPT, &lens);
        // Sanity: this really is a full, in-budget, non-shared cell (not accidentally empty).
        assert_eq!(
            re.total, 2,
            "the member set is {{x, y}} under {lens:?}: {re:?}"
        );
        assert_eq!(
            re.shared,
            Vec::<String>::new(),
            "no shared member under {lens:?}"
        );
        assert_eq!(re.truncated, None, "in budget under {lens:?}");
        assert_eq!(
            re.empty_state, None,
            "a full cell carries no message under {lens:?}"
        );

        let obj = serde_json::to_value(&re).unwrap();
        let obj = obj.as_object().unwrap().clone();
        for field in ["shared", "truncated", "empty_state", "unresolved"] {
            assert!(
                !obj.contains_key(field),
                "a defaulted `{field}` is omitted from the wire under {lens:?} (back-compat): {obj:?}"
            );
        }
        // The surviving keys are exactly the pre-criterion-2 shape.
        let mut present: Vec<&String> = obj.keys().collect();
        present.sort();
        assert_eq!(
            present,
            vec![&"clusters".to_string(), &"edges".to_string(), &"subject".to_string(), &"total".to_string()],
            "the defaulted body is the byte-identical pre-criterion-2 shape under {lens:?}: {obj:?}"
        );
    }
}

/// The same graph + subject + lens yields a BYTE-identical serialized body across re-projections
/// (the [`rigger::dash::Reprojection`] determinism contract) - even for a wide cell whose cap ranks
/// buckets and prunes edges. A non-deterministic rank or prune would make the panel flicker on poll.
#[test]
fn a_re_projection_serializes_byte_identically_across_calls() {
    // A wide cell exercises the ranked cap + edge prune, the parts most at risk of poll instability.
    let graph = per_file_graph(CLUSTER_RENDER_BUDGET, 3);

    let first = serde_json::to_vec(&reproject(&graph, WIDE_CONCEPT, &Lens::Files)).unwrap();
    let second = serde_json::to_vec(&reproject(&graph, WIDE_CONCEPT, &Lens::Files)).unwrap();
    assert_eq!(
        first, second,
        "a re-grain of one graph+subject+lens is byte-identical across polls"
    );

    // And a concepts re-grain with a shared flag is equally stable.
    let cg = shared_member_graph();
    let a = serde_json::to_vec(&reproject(&cg, SUB_C, &concepts_lens())).unwrap();
    let b = serde_json::to_vec(&reproject(&cg, SUB_C, &concepts_lens())).unwrap();
    assert_eq!(a, b, "a concepts re-grain is byte-identical across polls");
}

// A compile-time pin that the two documented empty-cell messages are the exact public constants the
// panel captions with (the mechanics tests bind them to behavior; this binds their literal values, so
// a silent edit to either string is caught at this boundary too).
#[test]
fn the_empty_cell_message_constants_hold_their_documented_values() {
    assert_eq!(REPROJECT_NO_COMMUNITY, "no derived communities");
    assert_eq!(REPROJECT_NO_CONCEPT, "not part of any concept");
    // And the constants are exactly what a memberless derived cell emits (constant <-> behavior).
    let graph = Graph {
        nodes: vec![
            node("concept/9/0", KIND_CONCEPT, Some("idea")),
            def("src/z.rs::only", "only"),
        ],
        edges: vec![edge("src/z.rs::only", "concept/9/0", REL_REALIZES)],
    };
    assert_eq!(
        reproject(&graph, "concept/9/0", &code_lens())
            .empty_state
            .as_deref(),
        Some(REPROJECT_NO_COMMUNITY),
        "a code re-grain with no community membership emits the community constant"
    );
}
