//! Periphery (contract / API) tests for the SHARED graph-assembly + detection surface spec 54 makes
//! public so the CODE lens and the CONCEPTS lens run ONE deterministic detection over either input
//! layer. Spec 54 extracts `Coupling::from_edges` (the single undirected-weighted-graph builder both
//! lenses feed), the `nodes()` / `degrees()` accessors the concept label picker reads, and
//! `partition` -> `Grouping` (the lens-agnostic detection result), then re-casts the code lens's
//! `detect` as a THIN ADAPTER over `partition`. Those are newly-`pub` and are the load-bearing cross-
//! lens seam, yet neither lens's own tests isolate them: the community suite drives `detect` /
//! `from_graph`, and the concepts suite drives `derive` / the binary. This file guards the shared
//! contract directly, over the library's public surface:
//!
//!  - `from_edges` ASSEMBLY: self-loops dropped, `a -> b` and `b -> a` collapse to one undirected
//!    edge, repeated occurrences accumulate weight (multiplicity), and the node vector is SORTED +
//!    unique with `degrees()` aligned index-for-index to `nodes()`.
//!  - `partition` DETERMINISM: two calls on the same layer at the same resolution return an equal
//!    `Grouping` (equal `group_of`, `hash`, `num_groups`), and `group_of` is aligned to `nodes()`.
//!  - the REFACTOR-AGREEMENT contract: `detect` is a faithful adapter over `partition` - its
//!    per-node community id is exactly `community/<res>/<partition.group_of[i]>`, its count equals
//!    `num_groups`, and its hash equals `partition`'s. A refactor that let `detect` drift from the
//!    shared `partition` (the seam the concepts lens depends on being identical) reddens here.
//!
//! Every symbol here is always-compiled, so these run identically in BOTH feature lanes.

use rigger::community::{self, Coupling};

/// Build a `Coupling` from string-pair edge occurrences (the `from_edges` input form both lenses
/// feed: the code lens its canonicalized coupling endpoints, the concepts lens its intent-edge
/// endpoints).
fn coupling(edges: &[(&str, &str)]) -> Coupling {
    Coupling::from_edges(
        edges
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect::<Vec<_>>(),
    )
}

/// A node's weighted degree via the public `nodes()` / `degrees()` alignment: `degrees()[i]` is the
/// degree of `nodes()[i]`, so a caller maps an id to its degree through the shared index.
fn degree_of(c: &Coupling, id: &str) -> f64 {
    let i = c
        .nodes()
        .iter()
        .position(|n| n == id)
        .expect("node present");
    c.degrees()[i]
}

#[test]
fn from_edges_drops_self_loops_collapses_direction_and_accumulates_multiplicity() {
    // A lone self-loop contributes no node - the assembly drops `from == to` before anything else.
    let only_self = coupling(&[("a", "a")]);
    assert!(
        only_self.is_empty(),
        "a lone self-loop yields an empty coupling; got nodes {:?}",
        only_self.nodes()
    );

    // Direction collapse: `b -> a` and `a -> b` are ONE undirected pair, so the node set is exactly
    // {a, b} - not a directional duplicate - and its weight accumulates (multiplicity).
    let c = coupling(&[("b", "a"), ("a", "b"), ("a", "c"), ("a", "a")]);
    assert_eq!(
        c.nodes(),
        &["a".to_string(), "b".to_string(), "c".to_string()],
        "nodes() is sorted + unique, the self-loop contributes nothing; got {:?}",
        c.nodes()
    );
    // Weighted degrees, aligned to nodes(): {a,b} accumulated weight 2 (two occurrences, collapsed
    // across direction), {a,c} weight 1. So deg(a)=3, deg(b)=2, deg(c)=1.
    assert_eq!(degree_of(&c, "a"), 3.0, "a: {{a,b}}=2 + {{a,c}}=1");
    assert_eq!(
        degree_of(&c, "b"),
        2.0,
        "b: the a<->b pair accumulated multiplicity 2"
    );
    assert_eq!(degree_of(&c, "c"), 1.0, "c: the single {{a,c}} edge");

    // nodes() and degrees() are the SAME length - the alignment the concept label picker relies on.
    assert_eq!(
        c.nodes().len(),
        c.degrees().len(),
        "nodes() and degrees() are index-aligned (same length)"
    );
}

#[test]
fn from_edges_is_direction_canonical_so_the_pass_hash_is_orientation_independent() {
    // The canonical rendering `from_edges` builds folds `a -> b` and `b -> a` to the SAME pair, so
    // the pass hash `partition` takes over the layer is orientation-independent: feeding the reversed
    // orientation yields the identical hash. This is what lets the two lenses build one deterministic
    // layer regardless of which way their source edges point.
    let forward = coupling(&[("a", "b"), ("b", "c")]);
    let reversed = coupling(&[("b", "a"), ("c", "b")]);
    assert_eq!(
        forward.nodes(),
        reversed.nodes(),
        "reversed orientation yields the identical node set"
    );
    assert_eq!(
        community::partition(&forward, community::DEFAULT_RESOLUTION).hash,
        community::partition(&reversed, community::DEFAULT_RESOLUTION).hash,
        "the pass hash is orientation-independent (direction canonicalized in the shared assembly)"
    );
}

#[test]
fn partition_is_deterministic_and_its_group_of_aligns_to_nodes() {
    // Two disconnected triangles: connectedness forces at least TWO groups (a group is internally
    // connected, so the components never fuse), which makes the alignment assertion non-vacuous.
    let c = coupling(&[
        ("x1", "x2"),
        ("x2", "x3"),
        ("x1", "x3"),
        ("y1", "y2"),
        ("y2", "y3"),
        ("y1", "y3"),
    ]);
    // Every triangle node has weighted degree 2 (incident to two unit-weight pairs).
    for id in ["x1", "x2", "x3", "y1", "y2", "y3"] {
        assert_eq!(degree_of(&c, id), 2.0, "{id} has triangle degree 2");
    }

    let g1 = community::partition(&c, community::DEFAULT_RESOLUTION);
    let g2 = community::partition(&c, community::DEFAULT_RESOLUTION);
    assert_eq!(
        g1.group_of, g2.group_of,
        "partition is deterministic: equal group_of across runs"
    );
    assert_eq!(g1.hash, g2.hash, "partition is deterministic: equal hash");
    assert_eq!(
        g1.num_groups, g2.num_groups,
        "partition is deterministic: equal group count"
    );

    assert_eq!(
        g1.group_of.len(),
        c.nodes().len(),
        "group_of is aligned to nodes() (one group number per node)"
    );
    assert!(
        g1.num_groups >= 2,
        "two disconnected triangles form at least two connected groups; got {}",
        g1.num_groups
    );
    // The two triangles land in DIFFERENT groups (connectedness never fuses disjoint components).
    let group_at = |id: &str| -> usize {
        let i = c.nodes().iter().position(|n| n == id).unwrap();
        g1.group_of[i]
    };
    assert_eq!(group_at("x1"), group_at("x3"), "triangle X is one group");
    assert_eq!(group_at("y1"), group_at("y3"), "triangle Y is one group");
    assert_ne!(
        group_at("x1"),
        group_at("y1"),
        "the two disjoint triangles are distinct groups"
    );
}

#[test]
fn detect_is_a_faithful_adapter_over_the_shared_partition() {
    // The refactor-agreement contract that guards the code lens after spec 54 re-cast `detect` as a
    // thin formatter over the shared `partition`. For the SAME coupling at the SAME resolution:
    //   - detect's per-node community id == `community/<res>/<partition.group_of[i]>`
    //   - detect.num_communities == partition.num_groups
    //   - detect.hash == partition.hash
    // A refactor bug that let `detect` drift from `partition` - the very seam the concepts lens
    // reuses verbatim - reddens here, even though every community fixture still passed.
    let c = coupling(&[
        ("x1", "x2"),
        ("x2", "x3"),
        ("x1", "x3"),
        ("y1", "y2"),
        ("y2", "y3"),
        ("y1", "y3"),
    ]);
    let res = community::DEFAULT_RESOLUTION;

    let grouping = community::partition(&c, res);
    let assignment = community::detect(&c, res);

    assert_eq!(
        assignment.hash, grouping.hash,
        "detect reports the shared partition's pass hash"
    );
    assert_eq!(
        assignment.num_communities, grouping.num_groups,
        "detect's community count is the shared partition's group count"
    );

    // detect.members is (node_id, community_id) sorted by node id; nodes() is sorted; so members[i]
    // aligns to nodes()[i] / group_of[i]. Each community id is the group number in the code lens's
    // `community/<res>/<n>` id space - the ONLY thing detect adds over partition.
    assert_eq!(
        assignment.members.len(),
        c.nodes().len(),
        "detect assigns exactly one community per node"
    );
    let res_str = format!("{res}");
    for (i, (node, community)) in assignment.members.iter().enumerate() {
        assert_eq!(
            node,
            &c.nodes()[i],
            "detect.members is aligned to the sorted node vector at index {i}"
        );
        assert_eq!(
            community,
            &format!("community/{res_str}/{}", grouping.group_of[i]),
            "detect formats the shared group number into the code lens id space for node {node}"
        );
    }
}
