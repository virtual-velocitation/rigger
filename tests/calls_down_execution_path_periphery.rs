//! Periphery (contract / API / integration) tests for spec 52 criterion 1: the DOWN direction of
//! `Projection::calls` - the layered execution-path DAG. These run OUTSIDE the crate, over the
//! library's PUBLIC surface (`Projector::open` -> `Projection::apply` -> `Projection::calls`), so
//! they guard the boundary the implementer's inside-out unit test is structurally blind to.
//!
//! The implementer's inside-out unit test
//! (`calls_down_walks_the_execution_path_as_a_layered_deduped_dag_with_a_back_edge`) owns the
//! directed walk itself: layers, single-candidate cross-file resolution, dedup under a cycle, and
//! the back-edge marker on a single small single-candidate graph. This layer deliberately does NOT
//! re-prove those. It covers the public boundaries that unit test leaves untouched:
//!
//!  - the `Projection::calls` trait DEFAULT: a projection with no directed-walk support degrades to
//!    an empty `CallGraph`, never an error (a backend-agnostic contract, in `mod contract`);
//!  - the multi-candidate FRONTIER: a callee name with two definitions comes back as a marked
//!    frontier carrying the SORTED candidate ids and is NOT descended (the public `frontier` field;
//!    the unit test uses only single-candidate hops, so it never enters this branch);
//!  - the TIER FLOOR: an ambiguous-tier call is EXCLUDED at the default (inferred) floor and
//!    INCLUDED only when the caller lowers the floor to `ambiguous` (the public `tier_floor` param);
//!  - the DEPTH clamp: the public `depth` bounds the layers the walk returns;
//!  - the `Direction::Up` arm (spec 52 criterion 3, now landed): over the SAME real call, UP resolves
//!    the caller back onto the seed's definition through its bare cross-file placeholder (the reverse
//!    name-match), where DOWN reaches the callee - the two directions are genuine mirror walks;
//!  - the DEGENERATE edges: a missing seed, and a seed with no calls, each degrade to an empty /
//!    seed-only view rather than erroring;
//!  - PROJECT SCOPING: a walk scoped to one project never surfaces another project's `CALLS` edges
//!    even when both share a graph.db file and the seed id (the new reads all filter on `project`).
//!
//! Events are built from raw on-log JSON (deliberately NOT the in-crate payload struct) so the tests
//! pin the JSON contract the fold reads, exactly as the sibling graph-fold periphery tests do. No
//! reference to any external tool or project; hyphens, never em dashes.

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    CallGraph, Direction, Projection, REL_CALLS, TIER_AMBIGUOUS, TIER_INFERRED,
    TYPE_CODE_ENTITY_EXTRACTED, TYPE_EDGE_INFERRED,
};
use rigger::eventstore::Event;

/// Fold a code DEFINITION (`file` defines the function `name` at `line`) from its raw on-log JSON at
/// `pos`. `fresh` marks the FIRST event of a file's extraction batch (it supersedes the file's prior
/// structural edges before folding). A successful `apply` is itself evidence the payload folded -
/// `apply` returns `Err` on a fold failure.
fn apply_def(p: &Projector, pos: u64, file: &str, name: &str, line: u32, fresh: bool) {
    let payload = serde_json::json!({
        "file": file, "name": name, "kind": "function", "line": line, "lang": "rust",
        "fresh": fresh,
    });
    let mut e = Event::new(
        TYPE_CODE_ENTITY_EXTRACTED,
        serde_json::to_vec(&payload).unwrap(),
    );
    e.position = pos;
    p.apply(&e).unwrap();
}

/// Fold a CALLER-ATTRIBUTED reference (spec 37): `file` references `name` from inside the enclosing
/// definition `caller` - exactly the event the emit pass produces for a call in a function body,
/// which the fold turns into `<file>::<caller> --CALLS--> <target>`. Built from raw JSON so the test
/// pins the on-log contract, not the Rust payload type.
fn apply_call(p: &Projector, pos: u64, file: &str, name: &str, caller: &str) {
    let payload = serde_json::json!({
        "file": file, "name": name, "lang": "rust", "caller": caller,
    });
    let mut e = Event::new(TYPE_EDGE_INFERRED, serde_json::to_vec(&payload).unwrap());
    e.position = pos;
    p.apply(&e).unwrap();
}

/// The reached node ids of a `CallGraph`, sorted, for a stable membership assertion.
fn node_ids(cg: &CallGraph) -> Vec<String> {
    let mut v: Vec<String> = cg.nodes.iter().map(|n| n.node.id.clone()).collect();
    v.sort();
    v
}

/// The `(from, to)` endpoints of a `CallGraph`'s edges, sorted.
fn edge_pairs(cg: &CallGraph) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = cg
        .edges
        .iter()
        .map(|e| (e.edge.from.clone(), e.edge.to.clone()))
        .collect();
    v.sort();
    v
}

/// The layer a node id carries in the result, if it is present.
fn layer_of(cg: &CallGraph, id: &str) -> Option<i64> {
    cg.nodes.iter().find(|n| n.node.id == id).map(|n| n.layer)
}

/// Backend-agnostic contract for the `Projection::calls` trait DEFAULT (spec 52). The default body
/// returns an empty `CallGraph`, so a projection with no directed-walk support - a test double, or a
/// not-yet-overriding adapter - degrades to an empty view rather than erroring. This is proven
/// WITHOUT the sqlite backend, so it guards the trait contract itself, independent of any one
/// implementor.
mod contract {
    use super::*;
    use rigger::contextgraph::{Error, Graph};

    /// A minimal `Projection` that answers the three required methods trivially and deliberately does
    /// NOT override `calls` - the stand-in for "a projection with no directed-walk support" the
    /// trait's own doc comment names. Leaving `calls` to the trait default is the whole point.
    struct NoDirectedWalk;

    impl Projection for NoDirectedWalk {
        fn apply(&self, _e: &Event) -> Result<(), Error> {
            Ok(())
        }
        fn subgraph(&self, _seed: &[String], _depth: i64) -> Result<Graph, Error> {
            Ok(Graph::default())
        }
        fn resolve(&self, _mention: &str) -> Result<Option<String>, Error> {
            Ok(None)
        }
    }

    #[test]
    fn a_projection_without_directed_walk_support_yields_an_empty_call_graph_in_either_direction() {
        let p = NoDirectedWalk;
        for dir in [Direction::Down, Direction::Up] {
            let cg = p
                .calls(&["any/file.rs::thing".to_string()], dir, 5, TIER_INFERRED)
                .expect("the default `calls` degrades to an empty view, never an error");
            assert!(
                cg.nodes.is_empty() && cg.edges.is_empty(),
                "the trait DEFAULT `calls` returns an empty CallGraph ({dir:?}); got nodes {:?} edges {:?}",
                super::node_ids(&cg),
                super::edge_pairs(&cg),
            );
        }
    }
}

#[test]
fn calls_down_surfaces_a_multi_candidate_hop_as_a_marked_frontier_and_never_descends_it() {
    // Spec 52 criterion 1's CONSERVATIVE resolution boundary, at the public API. A cross-file callee
    // whose name has MORE THAN ONE definition must come back as a marked frontier carrying its
    // SORTED candidate definition ids, and the walk must NOT descend into any candidate - honest by
    // construction (the view may be INCOMPLETE but never confidently wrong; the human re-seeds on a
    // chosen candidate). The implementer's unit test uses only single-candidate hops, so this
    // `frontier`-bearing branch of `resolve_down_hop` / `calls_down` is untouched by it.
    let p = Projector::open(":memory:", "test").unwrap();

    // `target` is defined in TWO files (a.rs and b.rs) - two candidates. c.rs's `caller` calls it as
    // a bare cross-file reference. Definitions are folded FIRST so the CALLS edge tiers INFERRED
    // (a cross-file definition exists), placing it at/above the default floor.
    apply_def(&p, 1, "src/a.rs", "target", 1, true);
    apply_def(&p, 2, "src/b.rs", "target", 1, true);
    apply_def(&p, 3, "src/c.rs", "caller", 1, true);
    apply_call(&p, 4, "src/c.rs", "target", "caller");

    let cg = p
        .calls(
            &["src/c.rs::caller".to_string()],
            Direction::Down,
            5,
            TIER_INFERRED,
        )
        .unwrap();

    // The bare cross-file callee `src/c.rs::target` is returned as a layer-1 FRONTIER carrying the
    // sorted candidate definition ids - NOT resolved to either candidate.
    let frontier = cg
        .nodes
        .iter()
        .find(|n| n.node.id == "src/c.rs::target")
        .expect("the multi-candidate callee is returned as a (frontier) node");
    assert_eq!(
        frontier.layer, 1,
        "the frontier callee sits at layer 1 (one hop from the seed)"
    );
    assert_eq!(
        frontier.frontier,
        Some(vec![
            "src/a.rs::target".to_string(),
            "src/b.rs::target".to_string()
        ]),
        "the frontier carries its candidate definition ids, SORTED by id",
    );

    // The candidates themselves are NEVER descended into - neither appears as a reached node, and
    // there is no layer-2 node at all.
    for cand in ["src/a.rs::target", "src/b.rs::target"] {
        assert!(
            !cg.nodes.iter().any(|n| n.node.id == cand),
            "candidate {cand} must NOT be descended into; nodes were {:?}",
            node_ids(&cg),
        );
    }
    assert!(
        cg.nodes.iter().all(|n| n.layer <= 1),
        "the walk stops at the frontier - no node is deeper than layer 1; nodes were {:?}",
        cg.nodes
            .iter()
            .map(|n| (n.node.id.clone(), n.layer))
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        node_ids(&cg),
        vec!["src/c.rs::caller", "src/c.rs::target"],
        "exactly the seed and the marked frontier callee",
    );
    // The one edge lands on the frontier placeholder and is a forward (non-back) CALLS edge.
    assert_eq!(
        edge_pairs(&cg),
        vec![(
            "src/c.rs::caller".to_string(),
            "src/c.rs::target".to_string()
        )],
        "the single edge points at the frontier, not at any candidate",
    );
    assert!(
        cg.edges.iter().all(|e| e.edge.rel == REL_CALLS && !e.back),
        "the edge to the frontier is a forward CALLS edge",
    );
}

#[test]
fn the_tier_floor_excludes_an_ambiguous_call_by_default_and_includes_it_only_when_opted_in() {
    // Spec 52's TIER FLOOR boundary at the public `tier_floor` param. A call whose callee is defined
    // NOWHERE the graph knows folds an `ambiguous`-tier CALLS edge. The default floor (inferred)
    // must EXCLUDE it - a directed walk defaults to the resolvable tiers; passing `ambiguous` opts
    // the unresolved tier in. The implementer's unit test always passes `inferred` over resolvable
    // edges, so it never exercises either side of the floor.
    let p = Projector::open(":memory:", "test").unwrap();
    apply_def(&p, 1, "src/c.rs", "caller", 1, true);
    // `nowheredef` is defined in no file: the CALLS edge tiers `ambiguous` (grep-visible only).
    apply_call(&p, 2, "src/c.rs", "nowheredef", "caller");

    // Default floor (inferred): the ambiguous edge is below the floor and is NOT followed - the walk
    // is just the seed, no edges.
    let default_floor = p
        .calls(
            &["src/c.rs::caller".to_string()],
            Direction::Down,
            5,
            TIER_INFERRED,
        )
        .unwrap();
    assert_eq!(
        node_ids(&default_floor),
        vec!["src/c.rs::caller"],
        "the ambiguous call is excluded at the default (inferred) floor - seed only",
    );
    assert!(
        default_floor.edges.is_empty(),
        "no edge is followed below the floor; edges were {:?}",
        edge_pairs(&default_floor),
    );

    // Floor lowered to `ambiguous`: the same walk now follows the edge to its terminal bare leaf.
    let ambiguous_floor = p
        .calls(
            &["src/c.rs::caller".to_string()],
            Direction::Down,
            5,
            TIER_AMBIGUOUS,
        )
        .unwrap();
    assert_eq!(
        node_ids(&ambiguous_floor),
        vec!["src/c.rs::caller", "src/c.rs::nowheredef"],
        "lowering the floor to `ambiguous` opts the unresolved tier in - the callee is reached",
    );
    assert_eq!(
        edge_pairs(&ambiguous_floor),
        vec![(
            "src/c.rs::caller".to_string(),
            "src/c.rs::nowheredef".to_string()
        )],
        "the previously-excluded ambiguous CALLS edge is now followed",
    );
    assert_eq!(
        layer_of(&ambiguous_floor, "src/c.rs::nowheredef"),
        Some(1),
        "the opted-in callee is a layer-1 terminal leaf (defined nowhere, nothing to descend)",
    );
}

#[test]
fn the_depth_bound_clamps_the_layers_the_walk_returns() {
    // Spec 52's `depth` param at the public API: the walk is bounded to `depth` hops. Build a linear
    // same-file chain f0 -> f1 -> f2 -> f3 and walk with depth 2: layers 0..=2 (f0, f1, f2) are
    // returned and f3 (which would be layer 3) is NOT, because a node at the bound is not expanded.
    let p = Projector::open(":memory:", "test").unwrap();
    apply_def(&p, 1, "src/a.rs", "f0", 1, true);
    apply_def(&p, 2, "src/a.rs", "f1", 2, false);
    apply_def(&p, 3, "src/a.rs", "f2", 3, false);
    apply_def(&p, 4, "src/a.rs", "f3", 4, false);
    apply_call(&p, 5, "src/a.rs", "f1", "f0");
    apply_call(&p, 6, "src/a.rs", "f2", "f1");
    apply_call(&p, 7, "src/a.rs", "f3", "f2");

    let cg = p
        .calls(
            &["src/a.rs::f0".to_string()],
            Direction::Down,
            2,
            TIER_INFERRED,
        )
        .unwrap();

    assert_eq!(
        node_ids(&cg),
        vec!["src/a.rs::f0", "src/a.rs::f1", "src/a.rs::f2"],
        "depth 2 returns exactly layers 0..=2; f3 (layer 3) is clamped out",
    );
    assert_eq!(layer_of(&cg, "src/a.rs::f2"), Some(2), "f2 is at the bound");
    assert!(
        !cg.nodes.iter().any(|n| n.node.id == "src/a.rs::f3"),
        "the node beyond the depth bound is not reached; nodes were {:?}",
        node_ids(&cg),
    );
    assert_eq!(
        edge_pairs(&cg),
        vec![
            ("src/a.rs::f0".to_string(), "src/a.rs::f1".to_string()),
            ("src/a.rs::f1".to_string(), "src/a.rs::f2".to_string()),
        ],
        "the edge OUT of the clamped node (f2 -> f3) is never traversed",
    );
}

#[test]
fn the_up_direction_mirrors_the_down_walk_resolving_the_caller_through_its_bare_placeholder() {
    // Spec 52 criterion 3 (now landed), over the library's PUBLIC surface: UP is the genuine mirror
    // of DOWN. `b.rs::caller` calls `callee`, which is DEFINED in `a.rs` - a CROSS-FILE call whose
    // edge lands on the bare `b.rs::callee` placeholder in the caller's own file namespace. DOWN from
    // the caller resolves that placeholder FORWARD onto the definition; UP from the definition
    // resolves it in REVERSE back onto the caller (the reverse name-match). A naive reverse walk that
    // only matched the literal edge target would connect neither direction across the file boundary.
    let p = Projector::open(":memory:", "test").unwrap();
    apply_def(&p, 1, "src/a.rs", "callee", 1, true);
    apply_def(&p, 2, "src/b.rs", "caller", 1, true);
    apply_call(&p, 3, "src/b.rs", "callee", "caller");

    // Control / DOWN: from the caller the walk resolves the bare cross-file placeholder forward onto
    // the real definition `a.rs::callee`.
    let down = p
        .calls(
            &["src/b.rs::caller".to_string()],
            Direction::Down,
            5,
            TIER_INFERRED,
        )
        .unwrap();
    assert!(
        down.nodes.iter().any(|n| n.node.id == "src/a.rs::callee"),
        "the DOWN walk resolves the cross-file callee onto its definition; nodes were {:?}",
        node_ids(&down),
    );

    // UP: over that SAME real call, from the definition the walk resolves the caller back through the
    // bare `b.rs::callee` placeholder - the mirror of DOWN. The caller sits at layer 1; the bare
    // placeholder is resolved away; the edge keeps the real CALLS direction (caller -> callee).
    let up = p
        .calls(
            &["src/a.rs::callee".to_string()],
            Direction::Up,
            5,
            TIER_INFERRED,
        )
        .unwrap();
    let caller = up
        .nodes
        .iter()
        .find(|n| n.node.id == "src/b.rs::caller")
        .expect("the UP walk resolves the cross-file caller through its bare placeholder");
    assert_eq!(
        caller.layer,
        1,
        "the caller is one hop up from the seed definition; nodes were {:?}",
        node_ids(&up),
    );
    assert!(
        !up.nodes.iter().any(|n| n.node.id == "src/b.rs::callee"),
        "the bare cross-file placeholder is resolved away, not returned; nodes were {:?}",
        node_ids(&up),
    );
    assert_eq!(
        edge_pairs(&up),
        vec![(
            "src/b.rs::caller".to_string(),
            "src/a.rs::callee".to_string()
        )],
        "the caller edge lands on the seed definition, in the real CALLS direction",
    );
}

#[test]
fn a_missing_seed_and_a_seed_with_no_calls_each_degrade_to_an_empty_view_never_an_error() {
    // Spec 52's degenerate boundary: "an empty graph or a seed with no calls degrades to an empty
    // view, never an error." A seed id that is not a node in the project yields a fully empty
    // CallGraph; a seed that exists but calls nothing yields just itself at layer 0.
    let p = Projector::open(":memory:", "test").unwrap();
    apply_def(&p, 1, "src/a.rs", "lonely", 1, true);

    // A seed that does not exist as a node: empty view, no error.
    let missing = p
        .calls(
            &["src/a.rs::ghost".to_string()],
            Direction::Down,
            5,
            TIER_INFERRED,
        )
        .expect("a missing seed degrades to an empty view, never an error");
    assert!(
        missing.nodes.is_empty() && missing.edges.is_empty(),
        "a missing seed yields a fully empty CallGraph; got nodes {:?}",
        node_ids(&missing),
    );

    // A real seed that calls nothing: itself at layer 0, no edges.
    let lonely = p
        .calls(
            &["src/a.rs::lonely".to_string()],
            Direction::Down,
            5,
            TIER_INFERRED,
        )
        .unwrap();
    assert_eq!(
        node_ids(&lonely),
        vec!["src/a.rs::lonely"],
        "a seed with no calls returns just itself",
    );
    assert_eq!(
        layer_of(&lonely, "src/a.rs::lonely"),
        Some(0),
        "the seed is layer 0"
    );
    assert!(
        lonely.edges.is_empty(),
        "a seed with no calls has no edges; edges were {:?}",
        edge_pairs(&lonely),
    );
}

#[test]
fn the_walk_is_project_scoped_and_never_surfaces_another_projects_calls_from_a_shared_graph_db() {
    // Spec 28 / spec 52 read isolation, proven across the module seam on a REAL shared graph.db. Two
    // projects live in ONE file. BOTH define `caller` in the same file (so the seed id exists in
    // each), but ONLY `other` records the call `caller -> callee`. A DOWN walk scoped to `test` must
    // seed on its own `caller` yet surface NONE of `other`'s CALLS edges - which exercises the
    // project filter on `calls_out` itself, not merely the seed lookup. The `applied(position)`
    // watermark is shared per file, so every event carries a globally-distinct position.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("graph.db");
    let path = path.to_str().unwrap();

    let test_p = Projector::open(path, "test").unwrap();
    let other_p = Projector::open(path, "other").unwrap();

    // `test` knows only the definition of `caller`; `other` knows caller, callee, and the call.
    apply_def(&test_p, 1, "src/x.rs", "caller", 1, true);
    apply_def(&other_p, 2, "src/x.rs", "caller", 1, true);
    apply_def(&other_p, 3, "src/x.rs", "callee", 2, false);
    apply_call(&other_p, 4, "src/x.rs", "callee", "caller");

    // The `test` walk seeds on its own `caller`, but `other`'s call edge is invisible to it.
    let scoped = test_p
        .calls(
            &["src/x.rs::caller".to_string()],
            Direction::Down,
            5,
            TIER_INFERRED,
        )
        .unwrap();
    assert_eq!(
        node_ids(&scoped),
        vec!["src/x.rs::caller"],
        "the project-scoped walk sees its own seed but NONE of another project's callees",
    );
    assert!(
        scoped.edges.is_empty(),
        "another project's CALLS edge never leaks into this walk; edges were {:?}",
        edge_pairs(&scoped),
    );

    // Control: in `other`, the SAME seed walks to the callee - proving the isolation above is scope,
    // not an empty graph.
    let control = other_p
        .calls(
            &["src/x.rs::caller".to_string()],
            Direction::Down,
            5,
            TIER_INFERRED,
        )
        .unwrap();
    assert_eq!(
        node_ids(&control),
        vec!["src/x.rs::callee", "src/x.rs::caller"],
        "control: the same seed in `other` reaches its callee",
    );
    assert_eq!(
        edge_pairs(&control),
        vec![(
            "src/x.rs::caller".to_string(),
            "src/x.rs::callee".to_string()
        )],
        "control: `other`'s call edge is present in its own scope",
    );
}
