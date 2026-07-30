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
//! The UP direction (spec 52 criterion 3) carries public boundaries the DOWN layer and the crate's
//! inside-out UP unit tests leave untested at the public surface; this layer adds them, over the SAME
//! `Projector::open` -> `apply` -> `calls(Direction::Up)` surface:
//!
//!  - the `referenced_not_called` PUBLIC FIELD: an UP walk reports, beside the traversed caller DAG,
//!    the FILE nodes that import / use the seed's name at file level but call it from no function -
//!    callers are excluded, the DOWN walk leaves the field empty, and a missing seed yields an empty
//!    field (never an error);
//!  - the UP FRONTIER: a cross-file caller whose call name has MULTIPLE definitions comes back a
//!    marked `frontier` carrying its SORTED candidate ids and is NOT ascended (the reverse of the DOWN
//!    conservative-resolution policy; the crate's UP unit test exercises it only inside the crate);
//!  - UP PROJECT SCOPING across the THREE project-filtered reads the UP walk adds (same-file callers,
//!    cross-file callers through bare placeholders, and the referenced-but-not-called file scan) - a
//!    leak in any one is a bug the DOWN scoping test cannot catch;
//!  - the UP DEPTH clamp and the DETERMINISTIC emitted order: a caller tree walked at a depth bound
//!    returns its in-bound layers in the production `(layer, id)` order, asserted on the raw node Vec
//!    (not a re-sorted copy) so the ordering itself is pinned;
//!  - the SAME-LAYER BACK edge: two sibling callers at one layer where one calls the other, pinning
//!    the `<=` boundary that marks a non-deepening caller edge back rather than ascending it twice.
//!
//! Events are built from raw on-log JSON (deliberately NOT the in-crate payload struct) so the tests
//! pin the JSON contract the fold reads, exactly as the sibling graph-fold periphery tests do. No
//! reference to any external tool or project; hyphens, never em dashes.

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    CallGraph, Direction, Projection, KIND_FILE, REL_CALLS, TIER_AMBIGUOUS, TIER_INFERRED,
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

/// Fold a FILE-LEVEL reference (a top-level `use` / import): `file` references `name` with NO
/// enclosing caller, exactly the event the emit pass produces for a use outside every function body.
/// The fold turns it into `<file> --REFERENCES--> <file>::<name>` with NO caller-attributed `CALLS`
/// twin (the caller arm is purely additive), so the referencing file is a "referenced but not called"
/// site. Built from raw JSON so the test pins the on-log contract, not the Rust payload type.
fn apply_ref(p: &Projector, pos: u64, file: &str, name: &str) {
    let payload = serde_json::json!({ "file": file, "name": name, "lang": "rust" });
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

#[test]
fn the_up_walk_lists_referenced_but_not_called_files_at_the_public_boundary_and_excludes_callers() {
    // Spec 52 criterion 3, the UP direction's distinctive PUBLIC field `CallGraph.referenced_not_called`
    // (new in this unit). Beside the traversed caller DAG, an UP walk reports the FILE nodes that
    // reference the seed's name at file level (a top-level import / use) but call it from NO function
    // within them - the who-uses-this sites the caller DAG deliberately excludes. Proven over the
    // library's PUBLIC surface; the implementer's inside-out unit test asserts it inside the crate.
    let p = Projector::open(":memory:", "test").unwrap();
    // Definitions first, so cross-file calls tier INFERRED (a definition exists) at/above the floor.
    apply_def(&p, 1, "src/a.rs", "target", 1, true); // the SEED
    apply_def(&p, 2, "src/a.rs", "local", 2, false);
    apply_def(&p, 3, "src/b.rs", "mid", 1, true);
    // Two CALLERS. A caller-attributed reference folds BOTH a file-level REFERENCES edge AND a
    // `<file>::<caller> --CALLS--> target` edge, so its file is a CALLER (in the DAG) - never a
    // "referenced but not called" site.
    apply_call(&p, 4, "src/a.rs", "target", "local"); // same-file caller
    apply_call(&p, 5, "src/b.rs", "target", "mid"); // cross-file caller
                                                    // Two IMPORT-ONLY files: a top-level reference with NO enclosing caller folds ONLY the file-level
                                                    // REFERENCES edge (no CALLS twin) - exactly the "referenced but not called" site. e.rs is folded
                                                    // BEFORE d.rs so the natural row order would be [e, d]; only the builder's sort yields [d, e].
    apply_ref(&p, 6, "src/e.rs", "target");
    apply_ref(&p, 7, "src/d.rs", "target");

    let up = p
        .calls(
            &["src/a.rs::target".to_string()],
            Direction::Up,
            5,
            TIER_INFERRED,
        )
        .unwrap();

    // The sidecar is EXACTLY the two import-only files, as FILE nodes, sorted by id.
    let refd: Vec<String> = up
        .referenced_not_called
        .iter()
        .map(|n| n.id.clone())
        .collect();
    assert_eq!(
        refd,
        vec!["src/d.rs".to_string(), "src/e.rs".to_string()],
        "referenced-but-not-called is exactly the import-only files, sorted by id",
    );
    assert!(
        up.referenced_not_called.iter().all(|n| n.kind == KIND_FILE),
        "every referenced-but-not-called entry is a FILE node; got {:?}",
        up.referenced_not_called
            .iter()
            .map(|n| (n.id.clone(), n.kind.clone()))
            .collect::<Vec<_>>(),
    );
    // A file that CALLS the seed is a caller, NEVER a referenced-but-not-called site.
    for caller_file in ["src/a.rs", "src/b.rs"] {
        assert!(
            !up.referenced_not_called.iter().any(|n| n.id == caller_file),
            "a file that CALLS the seed is a caller, not a referenced-but-not-called site: {caller_file}",
        );
    }
    // The traversed DAG really carries those callers themselves (so the sidecar is the SEPARATE,
    // non-traversed who-uses list, not merely a relabeling of the caller set).
    assert!(
        up.nodes.iter().any(|n| n.node.id == "src/a.rs::local")
            && up.nodes.iter().any(|n| n.node.id == "src/b.rs::mid"),
        "the traversed DAG carries the callers themselves; nodes were {:?}",
        node_ids(&up),
    );

    // DOWN over the SAME graph carries an EMPTY sidecar - referenced-but-not-called is UP-only.
    let down = p
        .calls(
            &["src/a.rs::target".to_string()],
            Direction::Down,
            5,
            TIER_INFERRED,
        )
        .unwrap();
    assert!(
        down.referenced_not_called.is_empty(),
        "the DOWN execution path never carries the referenced-but-not-called sidecar; got {:?}",
        down.referenced_not_called
            .iter()
            .map(|n| n.id.clone())
            .collect::<Vec<_>>(),
    );

    // A missing UP seed degrades to a fully empty view - empty nodes AND empty sidecar, never an error.
    let missing = p
        .calls(
            &["src/a.rs::ghost".to_string()],
            Direction::Up,
            5,
            TIER_INFERRED,
        )
        .expect("a missing UP seed degrades to an empty view, never an error");
    assert!(
        missing.nodes.is_empty() && missing.referenced_not_called.is_empty(),
        "a missing UP seed yields empty nodes and an empty sidecar; got nodes {:?} sidecar {:?}",
        node_ids(&missing),
        missing
            .referenced_not_called
            .iter()
            .map(|n| n.id.clone())
            .collect::<Vec<_>>(),
    );
}

#[test]
fn the_up_walk_surfaces_a_multi_candidate_caller_as_a_marked_frontier_and_never_ascends_it() {
    // Spec 52 criterion 3, the honest-by-construction half of the UP walk at the PUBLIC surface (the
    // reverse of the DOWN conservative-resolution policy the sibling DOWN periphery test proves). A
    // cross-file caller whose call name has MORE THAN ONE definition cannot be confidently attributed
    // to THIS seed: it comes back a marked FRONTIER carrying its SORTED candidate definition ids and
    // is NOT ascended - the walk never guesses; nothing reachable only by ascending past it appears.
    let p = Projector::open(":memory:", "test").unwrap();
    // `target` is defined in TWO files, so a cross-file call to it is multi-candidate. Fold e.rs BEFORE
    // a.rs so the natural (rowid) candidate order would be [e, a]; only the builder's ORDER BY id
    // yields the asserted [a, e] - the candidate sort has teeth.
    apply_def(&p, 1, "src/e.rs", "target", 1, true);
    apply_def(&p, 2, "src/a.rs", "target", 1, true); // the SEED
    apply_def(&p, 3, "src/f.rs", "amb", 1, true); // the ambiguous caller
    apply_def(&p, 4, "src/g.rs", "over", 1, true); // amb's own caller - must NOT be reached
    apply_call(&p, 5, "src/f.rs", "target", "amb"); // multi-candidate cross-file call
    apply_call(&p, 6, "src/g.rs", "amb", "over"); // reachable only by ascending past the frontier

    let up = p
        .calls(
            &["src/a.rs::target".to_string()],
            Direction::Up,
            5,
            TIER_INFERRED,
        )
        .unwrap();

    let amb = up
        .nodes
        .iter()
        .find(|n| n.node.id == "src/f.rs::amb")
        .expect("the ambiguous caller is returned as a marked frontier node");
    assert_eq!(
        amb.layer, 1,
        "the frontier caller is one hop up from the seed"
    );
    assert_eq!(
        amb.frontier,
        Some(vec![
            "src/a.rs::target".to_string(),
            "src/e.rs::target".to_string()
        ]),
        "the frontier carries its candidate definition ids SORTED by id, even though e.rs was folded first",
    );
    assert_eq!(
        up.nodes.iter().filter(|n| n.frontier.is_some()).count(),
        1,
        "exactly one node is a frontier - the multi-candidate caller",
    );
    // NOT ASCENDED: `over` (reachable only past the frontier) and the sibling definition are absent.
    for hidden in ["src/g.rs::over", "src/e.rs::target"] {
        assert!(
            !up.nodes.iter().any(|n| n.node.id == hidden),
            "{hidden} lies beyond the un-ascended frontier and must not be reached; nodes were {:?}",
            node_ids(&up),
        );
    }
    assert_eq!(
        node_ids(&up),
        vec!["src/a.rs::target", "src/f.rs::amb"],
        "the reached set is exactly the seed plus the marked frontier caller",
    );
    // The one caller edge keeps the real CALLS direction, landing on the seed definition.
    assert_eq!(
        edge_pairs(&up),
        vec![("src/f.rs::amb".to_string(), "src/a.rs::target".to_string())],
        "one caller edge, onto the seed definition, marking the frontier",
    );
}

#[test]
fn the_up_walk_is_project_scoped_across_a_shared_graph_db_including_the_referenced_not_called_reads(
) {
    // Spec 28 / spec 52 read isolation for the UP direction, on a REAL shared graph.db. The UP walk
    // issues THREE project-filtered reads a DOWN walk does not - same-file callers, cross-file callers
    // through bare placeholders, and the referenced-but-not-called file scan - so a leak in ANY of the
    // three is a bug the DOWN scoping test cannot catch. Two projects share ONE file; both define the
    // seed `target` (so the seed id exists in each), but only `other` records callers and an import.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("graph.db");
    let path = path.to_str().unwrap();
    let test_p = Projector::open(path, "test").unwrap();
    let other_p = Projector::open(path, "other").unwrap();

    // `test` knows only the seed definition. `other` knows the seed, a same-file caller, a cross-file
    // caller, and an import-only file - all three UP read sources populated. The per-file `applied`
    // watermark is shared, so every event carries a globally-distinct position.
    apply_def(&test_p, 1, "src/a.rs", "target", 1, true);
    apply_def(&other_p, 2, "src/a.rs", "target", 1, true);
    apply_def(&other_p, 3, "src/a.rs", "local", 2, false);
    apply_def(&other_p, 4, "src/b.rs", "mid", 1, true);
    apply_call(&other_p, 5, "src/a.rs", "target", "local"); // same-file caller (callers_direct)
    apply_call(&other_p, 6, "src/b.rs", "target", "mid"); // cross-file caller (callers_via_bare)
    apply_ref(&other_p, 7, "src/d.rs", "target"); // import-only (referenced_not_called)

    // The `test` UP walk seeds on its own `target` but sees NONE of `other`'s callers or imports.
    let scoped = test_p
        .calls(
            &["src/a.rs::target".to_string()],
            Direction::Up,
            5,
            TIER_INFERRED,
        )
        .unwrap();
    assert_eq!(
        node_ids(&scoped),
        vec!["src/a.rs::target"],
        "the project-scoped UP walk sees its own seed but NONE of another project's callers",
    );
    assert!(
        scoped.edges.is_empty(),
        "another project's caller edge never leaks into this walk; edges were {:?}",
        edge_pairs(&scoped),
    );
    assert!(
        scoped.referenced_not_called.is_empty(),
        "another project's file-level reference never leaks into the sidecar; got {:?}",
        scoped
            .referenced_not_called
            .iter()
            .map(|n| n.id.clone())
            .collect::<Vec<_>>(),
    );

    // Control: in `other` the SAME seed reaches BOTH callers and the import-only file - proving the
    // isolation above is scope, not an empty graph.
    let control = other_p
        .calls(
            &["src/a.rs::target".to_string()],
            Direction::Up,
            5,
            TIER_INFERRED,
        )
        .unwrap();
    assert_eq!(
        node_ids(&control),
        vec!["src/a.rs::local", "src/a.rs::target", "src/b.rs::mid"],
        "control: `other`'s seed reaches both its same-file and cross-file callers",
    );
    let refd: Vec<String> = control
        .referenced_not_called
        .iter()
        .map(|n| n.id.clone())
        .collect();
    assert_eq!(
        refd,
        vec!["src/d.rs".to_string()],
        "control: `other`'s import-only file is in its own sidecar",
    );
}

#[test]
fn the_up_walk_clamps_the_caller_dag_to_the_depth_bound_and_emits_a_deterministic_layered_order() {
    // Spec 52 criterion 3: the UP BFS has its OWN `depth` clamp (a separate function from the DOWN
    // walk), and the spec's determinism global constraint requires a byte-identical response across
    // polls. Build a caller tree - two callers at layer 1, one at layer 2, one at layer 3 - and walk
    // UP with depth 2: layers 0..=2 are returned, the layer-3 caller is clamped out, and the emitted
    // node Vec is in the production (layer, id) order (NOT a re-sorted view), pinning the sort itself.
    let p = Projector::open(":memory:", "test").unwrap();
    apply_def(&p, 1, "src/a.rs", "t0", 1, true); // the SEED
                                                 // `mid` is folded BEFORE `alt` so the natural insertion order within layer 1 would be [mid, alt];
                                                 // only the builder's (layer, id) sort yields the asserted [alt, mid] - the ordering has teeth.
    apply_def(&p, 2, "src/a.rs", "mid", 2, false);
    apply_def(&p, 3, "src/a.rs", "alt", 3, false);
    apply_def(&p, 4, "src/a.rs", "top", 4, false);
    apply_def(&p, 5, "src/a.rs", "over", 5, false);
    apply_call(&p, 6, "src/a.rs", "t0", "mid"); // mid -> t0 : layer 1
    apply_call(&p, 7, "src/a.rs", "t0", "alt"); // alt -> t0 : layer 1
    apply_call(&p, 8, "src/a.rs", "mid", "top"); // top -> mid : layer 2
    apply_call(&p, 9, "src/a.rs", "top", "over"); // over -> top : layer 3 (clamped at depth 2)

    let up = p
        .calls(
            &["src/a.rs::t0".to_string()],
            Direction::Up,
            2,
            TIER_INFERRED,
        )
        .unwrap();

    // The layer-3 caller is clamped out; the bound node (top, layer 2) is present but not expanded.
    assert!(
        !up.nodes.iter().any(|n| n.node.id == "src/a.rs::over"),
        "the caller beyond the depth bound is not reached; nodes were {:?}",
        node_ids(&up),
    );
    assert_eq!(
        layer_of(&up, "src/a.rs::top"),
        Some(2),
        "the layer-2 caller sits at the bound"
    );
    assert_eq!(layer_of(&up, "src/a.rs::alt"), Some(1), "a layer-1 caller");
    assert_eq!(
        layer_of(&up, "src/a.rs::mid"),
        Some(1),
        "the other layer-1 caller"
    );
    // The EMITTED node order is the production (layer, id) order - asserted on the raw Vec, not a
    // re-sorted copy, so reversing or dropping the builder's sort reddens this.
    let emitted: Vec<String> = up.nodes.iter().map(|n| n.node.id.clone()).collect();
    assert_eq!(
        emitted,
        vec![
            "src/a.rs::t0".to_string(),  // layer 0
            "src/a.rs::alt".to_string(), // layer 1, id-sorted before mid
            "src/a.rs::mid".to_string(), // layer 1
            "src/a.rs::top".to_string(), // layer 2
        ],
        "the UP walk emits its nodes in (layer, id) order, deterministically across polls",
    );
    // The edge OUT of the clamped node (over -> top) is never traversed.
    assert_eq!(
        edge_pairs(&up),
        vec![
            ("src/a.rs::alt".to_string(), "src/a.rs::t0".to_string()),
            ("src/a.rs::mid".to_string(), "src/a.rs::t0".to_string()),
            ("src/a.rs::top".to_string(), "src/a.rs::mid".to_string()),
        ],
        "exactly the three in-bound caller edges; the edge out of the clamped node is absent",
    );
}

#[test]
fn the_up_walk_marks_a_same_layer_caller_edge_as_a_back_edge() {
    // Spec 52 criterion 3: an edge whose discovered caller sits at a layer NOT DEEPER than the callee
    // it was found from is a BACK edge (recursion / mutual call), marked rather than ascended a second
    // time - the boundary is `<=`, so a SAME-LAYER caller edge must be marked back. Two sibling callers
    // both call the seed (both land at layer 1) and one ALSO calls the other: that same-layer edge is
    // the case a `<` boundary would miss. Proven over the public surface.
    let p = Projector::open(":memory:", "test").unwrap();
    apply_def(&p, 1, "src/a.rs", "t0", 1, true); // the SEED
    apply_def(&p, 2, "src/a.rs", "c1", 2, false);
    apply_def(&p, 3, "src/a.rs", "c2", 3, false);
    apply_call(&p, 4, "src/a.rs", "t0", "c1"); // c1 -> t0 : layer 1
    apply_call(&p, 5, "src/a.rs", "t0", "c2"); // c2 -> t0 : layer 1
    apply_call(&p, 6, "src/a.rs", "c2", "c1"); // c1 -> c2 : a SAME-LAYER (1 -> 1) edge

    let up = p
        .calls(
            &["src/a.rs::t0".to_string()],
            Direction::Up,
            5,
            TIER_INFERRED,
        )
        .unwrap();

    // Both callers sit at layer 1; the seed appears once at layer 0 (no duplication under the cycle).
    assert_eq!(
        layer_of(&up, "src/a.rs::c1"),
        Some(1),
        "c1 is a layer-1 caller"
    );
    assert_eq!(
        layer_of(&up, "src/a.rs::c2"),
        Some(1),
        "c2 is a layer-1 caller"
    );
    assert_eq!(
        node_ids(&up),
        vec!["src/a.rs::c1", "src/a.rs::c2", "src/a.rs::t0"],
        "the reached set is the seed and its two sibling callers, each once",
    );

    let back_of = |from: &str, to: &str| -> Option<bool> {
        up.edges
            .iter()
            .find(|e| e.edge.from == from && e.edge.to == to)
            .map(|e| e.back)
    };
    // The two forward caller edges onto the seed (layer 1 -> layer 0) are NOT back edges.
    assert_eq!(
        back_of("src/a.rs::c1", "src/a.rs::t0"),
        Some(false),
        "the c1 -> seed edge is forward"
    );
    assert_eq!(
        back_of("src/a.rs::c2", "src/a.rs::t0"),
        Some(false),
        "the c2 -> seed edge is forward"
    );
    // The SAME-LAYER edge (c1 -> c2, layer 1 -> layer 1) is marked BACK - the `<=` boundary a `<`
    // boundary would miss.
    assert_eq!(
        back_of("src/a.rs::c1", "src/a.rs::c2"),
        Some(true),
        "the same-layer caller edge is marked a back edge; edges were {:?}",
        edge_pairs(&up),
    );
    assert_eq!(
        edge_pairs(&up),
        vec![
            ("src/a.rs::c1".to_string(), "src/a.rs::c2".to_string()),
            ("src/a.rs::c1".to_string(), "src/a.rs::t0".to_string()),
            ("src/a.rs::c2".to_string(), "src/a.rs::t0".to_string()),
        ],
        "exactly the three caller edges",
    );
}
