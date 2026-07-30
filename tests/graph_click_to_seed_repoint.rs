//! Periphery (API / integration) tests for spec 43 criterion 5 - the INTERACTIVE half: a run-tree
//! unit click drives `GET /api/graph?seed=<unit>`, and with the KIND_UNIT node de-noised away a raw
//! unit-id seed resolves to no node. The route must therefore re-point that click OFF the (absent)
//! unit node and ONTO the unit's own content - the decisions its agents made and the findings its
//! reviewers drew, BOTH attributed to the unit by the emitting spawn stamped in `meta` (a reviewer
//! emits its finding through the same `rigger emit --spawn` path, so the stamp is always present) -
//! so the click still lands on a real, non-empty neighborhood rather than an empty KG panel.
//!
//! These drive the crate's PUBLIC surface as the dash inspector's HTTP client does: `rigger::dash`'s
//! `route` (the actual `/api/graph` endpoint the click crosses), `repoint_seed`, and `unit_seeds`,
//! folded over `rigger::contextgraph`'s `Projector`/`subgraph` exactly as production warms the graph.
//! The implementer's inside-out tests exercise the same logic through in-crate helpers; this layer
//! pins the observable contract at the module boundary a downstream consumer actually crosses, and
//! adds the cross-unit isolation boundary a single-unit unit test cannot reach. It intentionally does
//! NOT re-author the implementer's inside-out tests; it adds the outside-in periphery layer.

use std::collections::HashMap;

use rigger::conductor::META_SPAWN;
use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{Graph, Projection, KIND_UNIT};
use rigger::dash::{graph_seeds, repoint_seed, route, unit_seeds};
use rigger::eventstore::Event;

/// Build one event from its raw on-log JSON at `pos` - the shape the loop actually records.
fn ev(pos: u64, type_: &str, payload: serde_json::Value) -> Event {
    let mut e = Event::new(type_, serde_json::to_vec(&payload).unwrap());
    e.position = pos;
    e
}

/// Fold a run into a real projection and pre-fetch its subgraph EXACTLY as the dash does on open
/// (`graph_seeds` -> `subgraph` at depth 2), so the route below sees the same in-memory graph
/// production serves. Returns both so a test can drive `repoint_seed`/`route` over it.
fn fold_and_prefetch(run: &[Event]) -> Graph {
    let p = Projector::open(":memory:", "test").unwrap();
    for e in run {
        p.apply(e).unwrap();
    }
    p.subgraph(&graph_seeds(run), 2).unwrap()
}

/// Drive the real `/api/graph` endpoint for one seed and return the parsed JSON body. Asserts the
/// route answers 200 (the KG endpoint is read-only and always answers a click).
fn get_graph(run: &[Event], graph: &Graph, seed: &str) -> serde_json::Value {
    let r = route(
        "GET",
        &format!("/api/graph?seed={seed}"),
        run,
        graph,
        &[],
        &HashMap::new(),
        3,
        "rigger-run",
        "origin/main",
        &[],
    );
    assert_eq!(r.status, 200, "the KG route answers 200 for a click");
    serde_json::from_slice(&r.body).unwrap()
}

/// The set of node ids in a `/api/graph` response body.
fn node_ids(body: &serde_json::Value) -> std::collections::BTreeSet<String> {
    body["nodes"]
        .as_array()
        .expect("the response carries a nodes array")
        .iter()
        .map(|n| n["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_de_noised_unit_click_lands_on_a_real_neighborhood_through_the_route() {
    // A run in production shape: a unit, a decision its implementer emitted and a finding a reviewer
    // drew ABOUT the unit - BOTH stamped with their emitting spawn (`u1`'s implementer / `u1`'s sdet
    // lens), exactly as `rigger emit --spawn` records them. The finding carries no `$.unit` field; its
    // unit is the `meta.spawn` stamp, as in production.
    let run = vec![
        ev(
            1,
            "UnitStarted",
            serde_json::json!({ "unit": "u1", "criterion": "c", "agent": "impl", "needs": [] }),
        ),
        ev(
            2,
            "DecisionMade",
            serde_json::json!({ "id": "d1", "summary": "use the shared authority", "governs": ["combat.rs"], "supersedes": "" }),
        )
        .with_meta(META_SPAWN, "u1/implementer#0"),
        ev(
            3,
            "ReviewFinding",
            serde_json::json!({ "id": "f1", "by": "sdet", "summary": "y", "about": ["render.rs"] }),
        )
        .with_meta(META_SPAWN, "u1/lens:sdet#0"),
    ];

    let graph = fold_and_prefetch(&run);
    assert!(
        !graph.nodes.iter().any(|n| n.id == "u1"),
        "no KIND_UNIT node exists - the click-to-seed must re-point off the (gone) unit node"
    );
    assert!(
        !graph.nodes.iter().any(|n| n.kind == KIND_UNIT),
        "the machinery is gone: no KIND_UNIT node is in the pre-fetched graph"
    );

    // The interactive click: the run-tree renders the unit node with data-seed=<unit id>, so a click
    // fires GET /api/graph?seed=u1 - the raw unit id, which is no longer a node.
    let body = get_graph(&run, &graph, "u1");
    let ids = node_ids(&body);

    assert!(
        !ids.is_empty(),
        "the re-pointed unit click lands on a real, non-empty neighborhood, not an empty panel: {body}"
    );
    assert!(
        ids.contains("d1"),
        "the unit's decision (always a graph node) is in the clicked neighborhood: {body}"
    );
    assert!(
        ids.contains("f1"),
        "the unit's finding is in the clicked neighborhood: {body}"
    );
    assert!(
        ids.contains("combat.rs"),
        "the file the unit's decision governs is reached via its GOVERNS edge: {body}"
    );
    assert!(
        !ids.contains("u1"),
        "the unit id itself is never a node; the click landed via the unit's decisions/findings: {body}"
    );
    // graph_json echoes the seed the client selected even though the traversal fanned out from the
    // re-pointed content nodes - so `explain(<unit>)` and the client's selection state stay coherent.
    assert_eq!(
        body["seed"].as_str(),
        Some("u1"),
        "the response echoes the requested unit seed, not the re-pointed content ids: {body}"
    );
}

#[test]
fn a_unit_click_is_scoped_to_the_clicked_unit_and_never_drags_in_another() {
    // Two units, each with a decision and a finding - BOTH attributed to their unit by the emitting
    // spawn's `meta.spawn` (the production shape: a finding carries no `$.unit` field). Clicking `uA`
    // must land on ONLY uA's content - never uB's.
    let run = vec![
        ev(1, "DecisionMade", serde_json::json!({ "id": "dA", "summary": "x", "governs": ["a.rs"], "supersedes": "" }))
            .with_meta(META_SPAWN, "uA/implementer#0"),
        ev(2, "DecisionMade", serde_json::json!({ "id": "dB", "summary": "y", "governs": ["b.rs"], "supersedes": "" }))
            .with_meta(META_SPAWN, "uB/implementer#0"),
        ev(3, "ReviewFinding", serde_json::json!({ "id": "fA", "by": "sdet", "summary": "p", "about": ["x.rs"] }))
            .with_meta(META_SPAWN, "uA/lens:sdet#0"),
        ev(4, "ReviewFinding", serde_json::json!({ "id": "fB", "by": "sdet", "summary": "q", "about": ["y.rs"] }))
            .with_meta(META_SPAWN, "uB/lens:sdet#0"),
    ];

    // The public unit_seeds contract: ONLY the named unit's content ids + files, deterministically
    // sorted, never another unit's.
    assert_eq!(
        unit_seeds(&run, "uA"),
        vec![
            "a.rs".to_string(),
            "dA".to_string(),
            "fA".to_string(),
            "x.rs".to_string(),
        ],
        "unit_seeds returns exactly uA's content ids + files, sorted, and no uB content"
    );

    // The same scoping observed through the real route: clicking uA never drags in uB's neighborhood.
    let graph = fold_and_prefetch(&run);
    let ids = node_ids(&get_graph(&run, &graph, "uA"));
    for want in ["dA", "fA", "a.rs"] {
        assert!(ids.contains(want), "uA's click reaches {want}: {ids:?}");
    }
    for never in ["dB", "fB", "b.rs", "y.rs"] {
        assert!(
            !ids.contains(never),
            "uA's click never drags in uB's content ({never}): {ids:?}"
        );
    }
}

#[test]
fn repoint_seed_preserves_a_real_node_click_and_falls_back_gracefully() {
    // The public repoint_seed contract, all three arms, from a downstream consumer's vantage.
    let run = vec![
        ev(1, "DecisionMade", serde_json::json!({ "id": "d1", "summary": "x", "governs": ["combat.rs"], "supersedes": "" }))
            .with_meta(META_SPAWN, "u1/implementer#0"),
        ev(2, "ReviewFinding", serde_json::json!({ "id": "f1", "by": "sdet", "summary": "y", "about": ["render.rs"] }))
            .with_meta(META_SPAWN, "u1/lens:sdet#0"),
    ];
    let graph = fold_and_prefetch(&run);

    // Arm 1: a seed that already IS a graph node passes through UNCHANGED, so a spec-30 direct
    // node-click (e.g. clicking a decision) never regresses into a re-point.
    assert_eq!(
        repoint_seed(&run, &graph, "d1"),
        vec!["d1".to_string()],
        "a seed that resolves to a node is passed through unchanged"
    );

    // Arm 2: a unit-id seed (no longer a node) re-points onto that unit's content nodes, filtered to
    // ids that are real nodes in the pre-fetched graph - never the raw unit id.
    let repointed = repoint_seed(&run, &graph, "u1");
    assert!(
        repointed
            .iter()
            .all(|id| graph.nodes.iter().any(|n| &n.id == id)),
        "every re-pointed seed is a real node in the pre-fetched graph: {repointed:?}"
    );
    assert!(
        repointed.contains(&"d1".to_string()) && repointed.contains(&"f1".to_string()),
        "a unit seed re-points onto the unit's decision and finding: {repointed:?}"
    );
    assert!(
        !repointed.contains(&"u1".to_string()),
        "the re-point never carries the (absent) unit id: {repointed:?}"
    );

    // Arm 3: a seed that is neither a node nor a known unit falls back to the raw seed - never an
    // empty vec, so the route still answers a coherent (empty-neighborhood) 200 rather than crashing.
    assert_eq!(
        repoint_seed(&run, &graph, "no-such-thing"),
        vec!["no-such-thing".to_string()],
        "a seed matching neither a node nor a unit's content falls back to itself, never empty"
    );
}

#[test]
fn a_finding_is_attributed_only_by_its_emitting_spawn_never_a_stray_unit_field() {
    // The SINGLE-AUTHORITY boundary the fix established: a ReviewFinding attributes to its unit ONLY
    // by the emitting spawn stamped in `meta` (`spawn::unit_of`), NEVER by a `$.unit` event field.
    // Production findings emit through `rigger emit --spawn` and carry NO `unit` field, so an
    // attribution that keyed on that absent field silently omitted every real finding from a unit's
    // click. The other tests here prove the happy path (a field-less finding lands via meta.spawn);
    // this one pins the OTHER direction - a stray `unit` field is not a second, parallel authority.
    //
    // `fA` is emitted by uA's sdet lens (its true unit) but ALSO carries a misleading `unit: "uB"`.
    // Attribution must follow the emitting spawn (uA), never the stray field (uB).
    let run = vec![
        ev(1, "DecisionMade", serde_json::json!({ "id": "dA", "summary": "x", "governs": ["a.rs"], "supersedes": "" }))
            .with_meta(META_SPAWN, "uA/implementer#0"),
        ev(2, "ReviewFinding", serde_json::json!({ "id": "fA", "by": "sdet", "summary": "s", "about": ["x.rs"], "unit": "uB" }))
            .with_meta(META_SPAWN, "uA/lens:sdet#0"),
    ];

    // Public unit_seeds contract: fA belongs to uA (its emitting spawn), regardless of the stray field.
    assert!(
        unit_seeds(&run, "uA").contains(&"fA".to_string()),
        "a finding is attributed to its EMITTING spawn's unit (uA) by meta.spawn: {:?}",
        unit_seeds(&run, "uA")
    );
    // And NEVER to the unit named only by the stray `$.unit` field. If the dead field path resurrected,
    // fA (and its ABOUT file x.rs) would leak into uB's seeds - so this is the anti-regression pin.
    let ub_seeds = unit_seeds(&run, "uB");
    assert!(
        !ub_seeds.iter().any(|s| s == "fA" || s == "x.rs"),
        "the stray `unit` event field is not an attribution authority: uB's seeds carry no uA finding: {ub_seeds:?}"
    );

    // The same single authority observed through the real /api/graph route.
    let graph = fold_and_prefetch(&run);
    let ids_ua = node_ids(&get_graph(&run, &graph, "uA"));
    assert!(
        ids_ua.contains("fA"),
        "clicking uA surfaces its own finding fA (attributed by its emitting spawn): {ids_ua:?}"
    );
    let ids_ub = node_ids(&get_graph(&run, &graph, "uB"));
    assert!(
        !ids_ub.contains("fA") && !ids_ub.contains("x.rs"),
        "clicking uB never surfaces uA's finding via the stray `unit` field: {ids_ub:?}"
    );
}
