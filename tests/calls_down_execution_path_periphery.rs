//! Periphery (contract / API / integration) tests for spec 52 criterion 1: the DOWN direction of
//! `ContextGraph::calls` - the layered execution-path DAG. These run OUTSIDE the crate, over the
//! library's PUBLIC surface (source files -> the code store's view -> `ContextGraph::calls`), so
//! they guard the boundary the implementer's inside-out unit test is structurally blind to.
//!
//! The implementer's inside-out unit test
//! (`calls_down_walks_the_execution_path_as_a_layered_deduped_dag_with_a_back_edge`) owns the
//! directed walk itself: layers, single-candidate cross-file resolution, dedup under a cycle, and
//! the back-edge marker on a single small single-candidate graph. This layer deliberately does NOT
//! re-prove those. It covers the public boundaries that unit test leaves untouched:
//!
//!  - the `ContextGraph::calls` trait DEFAULT: a projection with no directed-walk support degrades to
//!    an empty `CallGraph`, never an error (a backend-agnostic contract, in `mod contract`);
//!  - the multi-candidate FRONTIER: a callee name with two definitions comes back as a marked
//!    frontier carrying the SORTED candidate ids and is NOT descended (the public `frontier` field;
//!    the unit test uses only single-candidate hops, so it never enters this branch);
//!  - the TIER FLOOR: an ambiguous-tier call is EXCLUDED at the default (inferred) floor and
//!    INCLUDED only when the caller lowers the floor to `ambiguous` (the public `tier_floor` param);
//!  - the DEPTH clamp: the public `depth` bounds the layers the walk returns;
//!  - the `Direction::Up` arm (spec 52 criterion 3, now landed): over the SAME real call, UP resolves
//!    the caller back onto the seed's definition through its bare placeholder (the reverse
//!    name-match), where DOWN reaches the callee - the two directions are genuine mirror walks;
//!  - the DEGENERATE edges: a missing seed, and a seed with no calls, each degrade to an empty /
//!    seed-only view rather than erroring.
//!
//! The UP direction (spec 52 criterion 3) carries public boundaries the DOWN layer and the crate's
//! inside-out UP unit tests leave untested at the public surface; this layer adds them, over the SAME
//! source files -> view -> `calls(Direction::Up)` surface:
//!
//!  - the `referenced_not_called` PUBLIC FIELD: an UP walk reports, beside the traversed caller DAG,
//!    the FILE nodes that import / use the seed's name at file level (or, a staged reference, from a
//!    definition within them) but call it from no function -
//!    callers are excluded, the DOWN walk leaves the field empty, and a missing seed yields an empty
//!    field (never an error);
//!  - the UP FRONTIER: a cross-file caller whose call name has MULTIPLE definitions comes back a
//!    marked `frontier` carrying its SORTED candidate ids and is NOT ascended (the reverse of the DOWN
//!    conservative-resolution policy; the crate's UP unit test exercises it only inside the crate);
//!  - the UP DEPTH clamp and the DETERMINISTIC emitted order: a caller tree walked at a depth bound
//!    returns its in-bound layers in the production `(layer, id)` order, asserted on the raw node Vec
//!    (not a re-sorted copy) so the ordering itself is pinned;
//!  - the SAME-LAYER BACK edge: two sibling callers at one layer where one calls the other, pinning
//!    the `<=` boundary that marks a non-deepening caller edge back rather than ascending it twice.
//!
//! Each scenario is written as Rust source and read through the code store's view, so the calls the
//! walk follows are the ones the extractor finds in the text. No reference to any external tool or
//! project; hyphens, never em dashes.

use crate::common;

use common::fixtures::{call_edge_pairs, call_layer, call_node_ids};
use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    CallGraph, ContextGraph, Direction, KIND_FILE, REL_CALLS, TIER_AMBIGUOUS, TIER_INFERRED,
};

/// The `direction` walk to `depth` from each of `seeds`, at the tier `floor`, over a tree holding
/// exactly `files` (path, Rust source) and no records.
fn walk(
    files: &[(&str, &str)],
    seeds: &[&str],
    direction: Direction,
    depth: i64,
    floor: &str,
) -> CallGraph {
    let (records, view) = tree(files);
    let seeds: Vec<String> = seeds.iter().map(|s| s.to_string()).collect();
    common::fixtures::view_graph(&records, &view)
        .calls(&seeds, direction, depth, floor)
        .unwrap()
}

/// The records (none) and the code store's view of a tree holding exactly `files`.
fn tree(files: &[(&str, &str)]) -> (Projector, rigger::code_view::FactsView) {
    (
        Projector::open(":memory:", "test").unwrap(),
        common::fixtures::files_view(&rigger_grounder::facts::FactsExtractor, files),
    )
}

/// The source of one file defining each of `defs`, one per line in order, where each `(caller,
/// callee)` of `calls` is a call inside the caller's body.
fn source(defs: &[&str], calls: &[(&str, &str)]) -> String {
    defs.iter()
        .map(|name| {
            let body: String = calls
                .iter()
                .filter(|(caller, _)| caller == name)
                .map(|(_, callee)| format!(" {callee}();"))
                .collect();
            format!("pub fn {name}() {{{body} }}\n")
        })
        .collect()
}

/// The `direction` walk to `depth` from `src/a.rs::<seed>` over one file, `src/a.rs`, that defines
/// each of `defs` in order and in which each `(caller, callee)` of `calls` is a same-file call.
fn walk_one_file(
    defs: &[&str],
    calls: &[(&str, &str)],
    seed: &str,
    direction: Direction,
    depth: i64,
) -> CallGraph {
    walk(
        &[("src/a.rs", &source(defs, calls))],
        &[&format!("src/a.rs::{seed}")],
        direction,
        depth,
        TIER_INFERRED,
    )
}

/// Backend-agnostic contract for the `ContextGraph::calls` trait DEFAULT (spec 52). The default body
/// returns an empty `CallGraph`, so a projection with no directed-walk support - a test double, or a
/// not-yet-overriding adapter - degrades to an empty view rather than erroring. This is proven
/// WITHOUT the sqlite backend, so it guards the trait contract itself, independent of any one
/// implementor.
mod contract {
    use super::*;
    use common::fixtures::MinimalGraph;
    use rigger::contextgraph::ContextGraph;

    #[test]
    fn a_projection_without_directed_walk_support_yields_an_empty_call_graph_in_either_direction() {
        let p = MinimalGraph;
        for dir in [Direction::Down, Direction::Up] {
            let cg = p
                .calls(&["any/file.rs::thing".to_string()], dir, 5, TIER_INFERRED)
                .expect("the default `calls` degrades to an empty view, never an error");
            assert!(
                cg.nodes.is_empty() && cg.edges.is_empty(),
                "the trait DEFAULT `calls` returns an empty CallGraph ({dir:?}); got nodes {:?} edges {:?}",
                super::call_node_ids(&cg),
                super::call_edge_pairs(&cg),
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

    // `target` is defined in TWO files (a.rs and b.rs) - two candidates. c.rs's `caller` calls it as
    // a bare cross-file reference. Definitions are folded FIRST so the CALLS edge tiers INFERRED
    // (a cross-file definition exists), placing it at/above the default floor.
    let (records, view) = tree(&[
        ("src/a.rs", "pub fn target() {}\n"),
        ("src/b.rs", "pub fn target() {}\n"),
        ("src/c.rs", "pub fn caller() { target(); }\n"),
    ]);
    let p = common::fixtures::view_graph(&records, &view);

    let cg = p
        .calls(
            &["src/c.rs::caller".to_string()],
            Direction::Down,
            5,
            TIER_INFERRED,
        )
        .unwrap();

    // The bare cross-file callee `src/c.rs::%target` is returned as a layer-1 FRONTIER carrying the
    // sorted candidate definition ids - NOT resolved to either candidate.
    let frontier = cg
        .nodes
        .iter()
        .find(|n| n.node.id == "src/c.rs::%target")
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
            call_node_ids(&cg),
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
        call_node_ids(&cg),
        vec!["src/c.rs::%target", "src/c.rs::caller"],
        "exactly the seed and the marked frontier callee",
    );
    // The one edge lands on the frontier placeholder and is a forward (non-back) CALLS edge.
    assert_eq!(
        call_edge_pairs(&cg),
        vec![(
            "src/c.rs::caller".to_string(),
            "src/c.rs::%target".to_string()
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
    // `nowheredef` is defined in no file: the CALLS edge tiers `ambiguous` (grep-visible only).

    // Default floor (inferred): the ambiguous edge is below the floor and is NOT followed - the walk
    // is just the seed, no edges.
    let (records, view) = tree(&[("src/c.rs", "pub fn caller() { nowheredef(); }\n")]);
    let p = common::fixtures::view_graph(&records, &view);

    let default_floor = p
        .calls(
            &["src/c.rs::caller".to_string()],
            Direction::Down,
            5,
            TIER_INFERRED,
        )
        .unwrap();
    assert_eq!(
        call_node_ids(&default_floor),
        vec!["src/c.rs::caller"],
        "the ambiguous call is excluded at the default (inferred) floor - seed only",
    );
    assert!(
        default_floor.edges.is_empty(),
        "no edge is followed below the floor; edges were {:?}",
        call_edge_pairs(&default_floor),
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
        call_node_ids(&ambiguous_floor),
        vec!["src/c.rs::%nowheredef", "src/c.rs::caller"],
        "lowering the floor to `ambiguous` opts the unresolved tier in - the callee is reached",
    );
    assert_eq!(
        call_edge_pairs(&ambiguous_floor),
        vec![(
            "src/c.rs::caller".to_string(),
            "src/c.rs::%nowheredef".to_string()
        )],
        "the previously-excluded ambiguous CALLS edge is now followed",
    );
    assert_eq!(
        call_layer(&ambiguous_floor, "src/c.rs::%nowheredef"),
        Some(1),
        "the opted-in callee is a layer-1 terminal leaf (defined nowhere, nothing to descend)",
    );
}

#[test]
fn the_depth_bound_clamps_the_layers_the_walk_returns() {
    // Spec 52's `depth` param at the public API: the walk is bounded to `depth` hops. Build a linear
    // same-file chain f0 -> f1 -> f2 -> f3 and walk with depth 2: layers 0..=2 (f0, f1, f2) are
    // returned and f3 (which would be layer 3) is NOT, because a node at the bound is not expanded.
    let cg = walk_one_file(
        &["f0", "f1", "f2", "f3"],
        &[("f0", "f1"), ("f1", "f2"), ("f2", "f3")],
        "f0",
        Direction::Down,
        2,
    );

    assert_eq!(
        call_node_ids(&cg),
        vec!["src/a.rs::f0", "src/a.rs::f1", "src/a.rs::f2"],
        "depth 2 returns exactly layers 0..=2; f3 (layer 3) is clamped out",
    );
    assert_eq!(
        call_layer(&cg, "src/a.rs::f2"),
        Some(2),
        "f2 is at the bound"
    );
    assert!(
        !cg.nodes.iter().any(|n| n.node.id == "src/a.rs::f3"),
        "the node beyond the depth bound is not reached; nodes were {:?}",
        call_node_ids(&cg),
    );
    assert_eq!(
        call_edge_pairs(&cg),
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
    // edge lands on the bare `b.rs::%callee` placeholder in the caller's own file namespace. DOWN
    // from the caller resolves that placeholder FORWARD onto the definition; UP from the definition
    // resolves it in REVERSE back onto the caller (the reverse name-match). A naive reverse walk that
    // only matched the literal edge target would connect neither direction across the file boundary.

    // Control / DOWN: from the caller the walk resolves the bare placeholder forward onto
    // the real definition `a.rs::callee`.
    let (records, view) = tree(&[
        ("src/a.rs", "pub fn callee() {}\n"),
        ("src/b.rs", "pub fn caller() { callee(); }\n"),
    ]);
    let p = common::fixtures::view_graph(&records, &view);

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
        call_node_ids(&down),
    );

    // UP: over that SAME real call, from the definition the walk resolves the caller back through the
    // bare `b.rs::%callee` placeholder - the mirror of DOWN. The caller sits at layer 1; the bare
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
        call_node_ids(&up),
    );
    assert!(
        !up.nodes.iter().any(|n| n.node.id == "src/b.rs::%callee"),
        "the bare placeholder is resolved away, not returned; nodes were {:?}",
        call_node_ids(&up),
    );
    assert_eq!(
        call_edge_pairs(&up),
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

    // A seed that does not exist as a node: empty view, no error.
    let (records, view) = tree(&[("src/a.rs", "pub fn lonely() {}\n")]);
    let p = common::fixtures::view_graph(&records, &view);

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
        call_node_ids(&missing),
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
        call_node_ids(&lonely),
        vec!["src/a.rs::lonely"],
        "a seed with no calls returns just itself",
    );
    assert_eq!(
        call_layer(&lonely, "src/a.rs::lonely"),
        Some(0),
        "the seed is layer 0"
    );
    assert!(
        lonely.edges.is_empty(),
        "a seed with no calls has no edges; edges were {:?}",
        call_edge_pairs(&lonely),
    );
}

#[test]
fn the_up_walk_lists_referenced_but_not_called_files_at_the_public_boundary_and_excludes_callers() {
    // Spec 52 criterion 3, the UP direction's distinctive PUBLIC field `CallGraph.referenced_not_called`
    // (new in this unit). Beside the traversed caller DAG, an UP walk reports the FILE nodes that
    // reference the seed's name at file level (a top-level import / use; a staged reference from a
    // definition within them is listed too, spec 117) but call it from NO function
    // within them - the who-uses-this sites the caller DAG deliberately excludes. Proven over the
    // library's PUBLIC surface; the implementer's inside-out unit test asserts it inside the crate.
    // Definitions first, so cross-file calls tier INFERRED (a definition exists) at/above the floor.
    // Two CALLERS. A caller-attributed reference folds BOTH a file-level REFERENCES edge AND a
    // `<file>::<caller> --CALLS--> target` edge, so its file is a CALLER (in the DAG) - never a
    // "referenced but not called" site.
    // Two IMPORT-ONLY files: a top-level reference with NO enclosing caller folds ONLY the file-level
    // REFERENCES edge (no CALLS twin) - exactly the "referenced but not called" site. e.rs is folded
    // BEFORE d.rs so the natural row order would be [e, d]; only the builder's sort yields [d, e].
    let (records, view) = tree(&[
        // the SEED, and its same-file caller
        (
            "src/a.rs",
            "pub fn target() {}\npub fn local() { target(); }\n",
        ),
        ("src/b.rs", "pub fn mid() { target(); }\n"), // cross-file caller
        ("src/e.rs", "use crate::a::target;\n"),
        ("src/d.rs", "use crate::a::target;\n"),
    ]);
    let p = common::fixtures::view_graph(&records, &view);

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
        call_node_ids(&up),
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
        call_node_ids(&missing),
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
    // `target` is defined in TWO files, so a cross-file call to it is multi-candidate. Fold e.rs BEFORE
    // a.rs so the natural (rowid) candidate order would be [e, a]; only the builder's ORDER BY id
    // yields the asserted [a, e] - the candidate sort has teeth.
    let (records, view) = tree(&[
        ("src/e.rs", "pub fn target() {}\n"),
        ("src/a.rs", "pub fn target() {}\n"), // the SEED
        // the ambiguous caller: a multi-candidate cross-file call
        ("src/f.rs", "pub fn amb() { target(); }\n"),
        // amb's own caller, reachable only by ascending past the frontier - must NOT be reached
        ("src/g.rs", "pub fn over() { amb(); }\n"),
    ]);
    let p = common::fixtures::view_graph(&records, &view);

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
            call_node_ids(&up),
        );
    }
    assert_eq!(
        call_node_ids(&up),
        vec!["src/a.rs::target", "src/f.rs::amb"],
        "the reached set is exactly the seed plus the marked frontier caller",
    );
    // The one caller edge keeps the real CALLS direction, landing on the seed definition.
    assert_eq!(
        call_edge_pairs(&up),
        vec![("src/f.rs::amb".to_string(), "src/a.rs::target".to_string())],
        "one caller edge, onto the seed definition, marking the frontier",
    );
}

#[test]
fn the_up_walk_clamps_the_caller_dag_to_the_depth_bound_and_emits_a_deterministic_layered_order() {
    // Spec 52 criterion 3: the UP BFS has its OWN `depth` clamp (a separate function from the DOWN
    // walk), and the spec's determinism global constraint requires a byte-identical response across
    // polls. Build a caller tree - two callers at layer 1, one at layer 2, one at layer 3 - and walk
    // UP with depth 2: layers 0..=2 are returned, the layer-3 caller is clamped out, and the emitted
    // node Vec is in the production (layer, id) order (NOT a re-sorted view), pinning the sort itself.
    let up = walk_one_file(
        // `t0` is the SEED. `mid` is folded BEFORE `alt` so the natural insertion order within
        // layer 1 would be [mid, alt]; only the builder's (layer, id) sort yields the asserted
        // [alt, mid] - the ordering has teeth.
        &["t0", "mid", "alt", "top", "over"],
        &[
            ("mid", "t0"),   // layer 1
            ("alt", "t0"),   // layer 1
            ("top", "mid"),  // layer 2
            ("over", "top"), // layer 3 (clamped at depth 2)
        ],
        "t0",
        Direction::Up,
        2,
    );

    // The layer-3 caller is clamped out; the bound node (top, layer 2) is present but not expanded.
    assert!(
        !up.nodes.iter().any(|n| n.node.id == "src/a.rs::over"),
        "the caller beyond the depth bound is not reached; nodes were {:?}",
        call_node_ids(&up),
    );
    assert_eq!(
        call_layer(&up, "src/a.rs::top"),
        Some(2),
        "the layer-2 caller sits at the bound"
    );
    assert_eq!(
        call_layer(&up, "src/a.rs::alt"),
        Some(1),
        "a layer-1 caller"
    );
    assert_eq!(
        call_layer(&up, "src/a.rs::mid"),
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
        call_edge_pairs(&up),
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
    let (records, view) = tree(&[(
        "src/a.rs",
        &source(
            &["t0", "c1", "c2"],
            // c1 -> t0 and c2 -> t0 at layer 1, and c1 -> c2 a SAME-LAYER (1 -> 1) edge
            &[("c1", "t0"), ("c2", "t0"), ("c1", "c2")],
        ),
    )]);
    let p = common::fixtures::view_graph(&records, &view);

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
        call_layer(&up, "src/a.rs::c1"),
        Some(1),
        "c1 is a layer-1 caller"
    );
    assert_eq!(
        call_layer(&up, "src/a.rs::c2"),
        Some(1),
        "c2 is a layer-1 caller"
    );
    assert_eq!(
        call_node_ids(&up),
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
        call_edge_pairs(&up),
    );
    assert_eq!(
        call_edge_pairs(&up),
        vec![
            ("src/a.rs::c1".to_string(), "src/a.rs::c2".to_string()),
            ("src/a.rs::c1".to_string(), "src/a.rs::t0".to_string()),
            ("src/a.rs::c2".to_string(), "src/a.rs::t0".to_string()),
        ],
        "exactly the three caller edges",
    );
}
