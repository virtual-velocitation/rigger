//! Periphery (contract / API / integration) tests for spec 54's EVENT-SOURCED FOLD: a
//! `ConceptDerived` event folds into a `KIND_CONCEPT` super-node carrying its pass-computed label,
//! and a `ConceptRealized` event folds into a live `<member> --REALIZES--> <concept>` edge at
//! `TIER_INFERRED`. These run OUTSIDE the crate, over the library's PUBLIC surface
//! (`rigger::contextgraph::...` + `Projector::apply` / `Projector::whole`), so they guard the
//! boundaries the inside-out fold / rebuild unit test (`src/concepts.rs`, a single non-superseding
//! pass) is structurally blind to:
//!
//!  - the SERIALIZED-FORM back-compat contract. `label`, `resolution`, and `hash` are all
//!    `#[serde(default)]` (and `fresh` is `#[serde(default)]` + `skip_serializing_if`), so a
//!    historical event recorded before those fields existed - or a minimal `{concept}` /
//!    `{node, concept}` emit - must still fold, and a `ConceptDerived` with NO `fresh` key must fold
//!    as a NON-boundary that never supersedes. The inside-out `events()` helper always supplies every
//!    field, so it never exercises the default arms; these tests drive the raw on-log JSON, the form
//!    a rebuild actually replays, deliberately bypassing the in-crate payload struct to pin the JSON
//!    contract rather than the Rust type.
//!  - the PUBLIC-CONST + TIER contract of the fold. An external consumer reads the projected concept
//!    layer as `KIND_CONCEPT` nodes joined by `REL_REALIZES` edges folded at `TIER_INFERRED` (a
//!    derived grouping one confidence step below the explicit intent edges it is detected over); the
//!    inside-out test asserts the rel but never the tier.
//!  - the persisted LITERAL forms. The event type on the log and the node kind / edge rel in the
//!    graph db are replayed on every rebuild; a drift silently orphans historical events and rows.
//!  - the `fresh` PASS-BOUNDARY supersession as a rebuild sees it: a re-run at a grain retires that
//!    grain's prior `REALIZES` edges AND drops its now-orphan `KIND_CONCEPT` nodes (no ghost bucket
//!    with a stale label and zero members), while leaving every OTHER resolution grain untouched.
//!  - the PROJECT scope of that supersession (spec 28): a `fresh` boundary in one project never
//!    retires an edge or drops a concept node belonging to another project on a shared backend.

use std::collections::BTreeSet;

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    Graph, Projection, KIND_CONCEPT, REL_REALIZES, TIER_INFERRED, TYPE_CONCEPT_DERIVED,
    TYPE_CONCEPT_REALIZED,
};
use rigger::eventstore::Event;

/// Fold an event built from its raw on-log JSON bytes at `pos` - the SERIALIZED form a rebuild
/// replays - into `p`, deliberately bypassing the in-crate payload structs so a test pins the JSON
/// contract, not the Rust type. `apply` returns `Err` on a deserialize failure, so a successful call
/// is itself evidence the payload satisfied the fold's contract.
fn apply_json(p: &Projector, pos: u64, type_: &str, json: serde_json::Value) {
    let mut e = Event::new(type_, serde_json::to_vec(&json).unwrap());
    e.position = pos;
    p.apply(&e).unwrap();
}

/// Fold one `ConceptDerived` (the concept super-node) at `pos`.
fn derived(p: &Projector, pos: u64, concept: &str, res: f64, fresh: bool) {
    apply_json(
        p,
        pos,
        TYPE_CONCEPT_DERIVED,
        serde_json::json!({
            "concept": concept, "label": "L", "resolution": res, "hash": "h", "fresh": fresh
        }),
    );
}

/// Fold one `ConceptRealized` (a `<node> --REALIZES--> <concept>` membership) at `pos`.
fn realized(p: &Projector, pos: u64, node: &str, concept: &str, res: f64) {
    apply_json(
        p,
        pos,
        TYPE_CONCEPT_REALIZED,
        serde_json::json!({ "node": node, "concept": concept, "resolution": res, "hash": "h" }),
    );
}

/// The live `REALIZES` targets of `member` in `g` (whole() returns only live edges), as a set.
fn live_realizes(g: &Graph, member: &str) -> BTreeSet<String> {
    g.edges
        .iter()
        .filter(|e| e.rel == REL_REALIZES && e.from == member)
        .map(|e| e.to.clone())
        .collect()
}

/// Every live `KIND_CONCEPT` node id in `g`, sorted.
fn concept_nodes(g: &Graph) -> Vec<String> {
    let mut ids: Vec<String> = g
        .nodes
        .iter()
        .filter(|n| n.kind == KIND_CONCEPT)
        .map(|n| n.id.clone())
        .collect();
    ids.sort();
    ids
}

#[test]
fn a_minimal_concept_realized_event_folds_a_live_membership_edge_backcompat() {
    // Serialized-form back-compat: the on-log form a pre-`resolution`/`hash` emit would have written
    // - only `node` and `concept`, no other key. Both later fields are `#[serde(default)]`, so this
    // must still fold a live `REALIZES` edge; the inside-out `events()` helper always supplies them,
    // so these default arms are untested by it. A rebuild that replays a pre-field log must not error.
    let p = Projector::open(":memory:", "test").unwrap();
    apply_json(
        &p,
        1,
        TYPE_CONCEPT_REALIZED,
        // Deliberately minimal: no resolution / hash key.
        serde_json::json!({ "node": "docs/a.md", "concept": "concept/1/0" }),
    );

    let g = p.whole().unwrap();
    assert!(
        g.edges.iter().any(|e| e.rel == REL_REALIZES
            && e.from == "docs/a.md"
            && e.to == "concept/1/0"
            && e.valid_to.is_none()),
        "a minimal ConceptRealized still folds a live REALIZES edge; got {:?}",
        g.edges
    );
    // The membership defensively ensured its concept super-node so the edge never dangles.
    assert!(
        g.nodes
            .iter()
            .any(|n| n.id == "concept/1/0" && n.kind == KIND_CONCEPT),
        "the membership fold ensured the KIND_CONCEPT super-node; got {:?}",
        g.nodes
    );
}

#[test]
fn a_minimal_concept_derived_event_folds_with_defaulted_attrs_backcompat() {
    // Serialized-form back-compat: a `ConceptDerived` carrying only its required `concept` id - no
    // label / resolution / hash / fresh key. Every other field is `#[serde(default)]`, so this must
    // still fold a KIND_CONCEPT node with deterministically defaulted attrs (never a fold error), and
    // the absent `fresh` folds as a NON-boundary.
    let p = Projector::open(":memory:", "test").unwrap();
    apply_json(
        &p,
        1,
        TYPE_CONCEPT_DERIVED,
        serde_json::json!({ "concept": "concept/0/0" }),
    );

    let g = p.whole().unwrap();
    let c = g
        .nodes
        .iter()
        .find(|n| n.id == "concept/0/0")
        .expect("a minimal ConceptDerived still folds a concept node");
    assert_eq!(
        c.kind, KIND_CONCEPT,
        "the minimal event still folds a KIND_CONCEPT super-node; got {c:?}"
    );
    assert_eq!(
        c.attrs.get("resolution").map(String::as_str),
        Some("0"),
        "a missing resolution defaults to the f64 default 0.0, stored as \"0\"; got {:?}",
        c.attrs
    );
    assert_eq!(
        c.attrs.get("hash").map(String::as_str),
        Some(""),
        "a missing hash defaults to the empty String, never a fold error; got {:?}",
        c.attrs
    );
    assert_eq!(
        c.attrs.get("label").map(String::as_str),
        Some(""),
        "a missing label defaults to the empty String; got {:?}",
        c.attrs
    );
}

#[test]
fn the_concept_fold_exposes_kind_relation_and_inferred_tier_over_the_public_surface() {
    // The public-const + tier contract as an external consumer reads it: the concept super-node is a
    // KIND_CONCEPT node carrying its pass-computed label, its members are joined by REL_REALIZES
    // edges, and those edges fold at TIER_INFERRED - a derived grouping one confidence step below the
    // explicit intent edges the derivation runs over. The inside-out test asserts the rel but never
    // the tier or the label attr.
    let p = Projector::open(":memory:", "test").unwrap();
    apply_json(
        &p,
        1,
        TYPE_CONCEPT_DERIVED,
        serde_json::json!({ "concept": "concept/1/0", "label": "The knowledge graph", "resolution": 1.0, "hash": "h", "fresh": true }),
    );
    realized(&p, 2, "docs/kg.md", "concept/1/0", 1.0);
    realized(&p, 3, "src/graph/store.rs", "concept/1/0", 1.0);

    let g = p.whole().unwrap();
    let c = g
        .nodes
        .iter()
        .find(|n| n.id == "concept/1/0")
        .expect("the concept super-node folded");
    assert_eq!(
        c.kind, KIND_CONCEPT,
        "the super-node folds at KIND_CONCEPT; got {c:?}"
    );
    assert_eq!(
        c.attrs.get("label").map(String::as_str),
        Some("The knowledge graph"),
        "the pass-computed label rides the event onto the concept node; got {:?}",
        c.attrs
    );
    let mem = g
        .edges
        .iter()
        .find(|e| e.rel == REL_REALIZES && e.from == "docs/kg.md" && e.to == "concept/1/0")
        .expect("a membership edge ties the doc member to its concept");
    assert!(
        mem.valid_to.is_none(),
        "the membership edge is live; got {mem:?}"
    );
    assert_eq!(
        mem.tier, TIER_INFERRED,
        "the derived membership edge folds at TIER_INFERRED (one step below its intent inputs); got {mem:?}"
    );
}

#[test]
fn a_concept_derived_event_without_a_fresh_key_is_a_non_boundary_and_never_supersedes() {
    // Serialized-form back-compat: `fresh` is `#[serde(default)]` + `skip_serializing_if`, so an
    // on-log `ConceptDerived` with NO `fresh` key folds as a NON-boundary - it never supersedes prior
    // memberships, it accretes. The inside-out `events()` helper always writes `fresh` on the first
    // event, so the absent-key path is untested by it. A pre-`fresh` log must replay without retiring
    // anything (the pass-boundary supersession is opt-in, gated on the emitter setting `fresh`).
    let p = Projector::open(":memory:", "test").unwrap();
    // First membership of node `x`, established WITHOUT a `fresh` key (the pre-field on-log form).
    apply_json(
        &p,
        1,
        TYPE_CONCEPT_DERIVED,
        serde_json::json!({ "concept": "concept/1/0", "label": "A", "resolution": 1.0, "hash": "h" }),
    );
    apply_json(
        &p,
        2,
        TYPE_CONCEPT_REALIZED,
        serde_json::json!({ "node": "x", "concept": "concept/1/0", "resolution": 1.0, "hash": "h" }),
    );
    // A second resolution-1.0 concept, ALSO with no `fresh` key: a non-boundary fold, so it must
    // accrete rather than replace - BOTH memberships stay live.
    apply_json(
        &p,
        3,
        TYPE_CONCEPT_DERIVED,
        serde_json::json!({ "concept": "concept/1/1", "label": "B", "resolution": 1.0, "hash": "h" }),
    );
    apply_json(
        &p,
        4,
        TYPE_CONCEPT_REALIZED,
        serde_json::json!({ "node": "x", "concept": "concept/1/1", "resolution": 1.0, "hash": "h" }),
    );

    let g = p.whole().unwrap();
    let live = live_realizes(&g, "x");
    let expected: BTreeSet<String> = ["concept/1/0".to_string(), "concept/1/1".to_string()]
        .into_iter()
        .collect();
    assert_eq!(
        live, expected,
        "with no `fresh` boundary a re-run accretes: both memberships stay live; got {live:?}"
    );
}

#[test]
fn the_concept_layer_serialized_literals_are_stable_and_the_fold_matches_the_on_log_type() {
    // These strings are PERSISTED (the event types on the log; the node kind and edge rel in the
    // graph db) and REPLAYED on every rebuild, so a drift silently orphans historical events / rows.
    assert_eq!(TYPE_CONCEPT_DERIVED, "ConceptDerived");
    assert_eq!(TYPE_CONCEPT_REALIZED, "ConceptRealized");
    assert_eq!(KIND_CONCEPT, "concept");
    assert_eq!(REL_REALIZES, "REALIZES");

    // Historical events whose raw on-log type literals are "ConceptDerived" / "ConceptRealized"
    // (bypassing the consts entirely) must still hit the fold arms - the arms match the persisted
    // strings, not renamed consts - and must store the literal `concept` kind / `REALIZES` rel a
    // later read expects.
    let p = Projector::open(":memory:", "test").unwrap();
    apply_json(
        &p,
        1,
        "ConceptDerived",
        serde_json::json!({ "concept": "concept/1/0", "label": "L", "resolution": 1.0, "hash": "h", "fresh": true }),
    );
    apply_json(
        &p,
        2,
        "ConceptRealized",
        serde_json::json!({ "node": "docs/a.md", "concept": "concept/1/0", "resolution": 1.0, "hash": "h" }),
    );

    let g = p.whole().unwrap();
    assert!(
        g.nodes
            .iter()
            .any(|n| n.id == "concept/1/0" && n.kind == "concept"),
        "the literal-typed event folds a node stored with the literal `concept` kind; got {:?}",
        g.nodes
    );
    assert!(
        g.edges
            .iter()
            .any(|e| e.rel == "REALIZES" && e.from == "docs/a.md" && e.to == "concept/1/0"),
        "the membership edge is stored with the literal `REALIZES` rel; got {:?}",
        g.edges
    );
}

#[test]
fn a_fresh_rerun_supersedes_its_grains_memberships_and_drops_the_now_orphan_concept_nodes() {
    // The `fresh` pass boundary as a rebuild sees it. Pass 1 at r=1 groups into TWO concepts; pass 2
    // at r=1 re-groups the SAME members into ONE. Pass 2's first `ConceptDerived` carries `fresh`, so
    // the fold must (a) retire every live `REALIZES` edge of grain `concept/1/` and (b) DROP the
    // grain's now-orphan concept nodes - so the vanished `concept/1/1` leaves NO ghost bucket (a
    // KIND_CONCEPT node with a stale label and zero live members), and every member realizes exactly
    // the one surviving concept. Neither the inside-out single-pass rebuild (no supersession) nor the
    // CLI supersession test (which never asserts the orphan-node drop) guards this.
    let p = Projector::open(":memory:", "test").unwrap();

    // Pass 1: concept/1/0 = {docs/a.md, src/a.rs}; concept/1/1 = {docs/b.md, src/b.rs}.
    derived(&p, 1, "concept/1/0", 1.0, true);
    derived(&p, 2, "concept/1/1", 1.0, false);
    realized(&p, 3, "docs/a.md", "concept/1/0", 1.0);
    realized(&p, 4, "src/a.rs", "concept/1/0", 1.0);
    realized(&p, 5, "docs/b.md", "concept/1/1", 1.0);
    realized(&p, 6, "src/b.rs", "concept/1/1", 1.0);

    let g1 = p.whole().unwrap();
    assert_eq!(
        concept_nodes(&g1),
        vec!["concept/1/0".to_string(), "concept/1/1".to_string()],
        "pass 1 materialized both concept nodes; got {:?}",
        concept_nodes(&g1)
    );

    // Pass 2: everything re-groups into concept/1/0. The fresh head retires grain-1's memberships and
    // drops concept/1/1 as an orphan before this pass's concepts re-add.
    derived(&p, 7, "concept/1/0", 1.0, true);
    realized(&p, 8, "docs/a.md", "concept/1/0", 1.0);
    realized(&p, 9, "src/a.rs", "concept/1/0", 1.0);
    realized(&p, 10, "docs/b.md", "concept/1/0", 1.0);
    realized(&p, 11, "src/b.rs", "concept/1/0", 1.0);

    let g2 = p.whole().unwrap();
    // The vanished concept left no ghost node.
    assert_eq!(
        concept_nodes(&g2),
        vec!["concept/1/0".to_string()],
        "the re-run dropped the now-orphan concept/1/1 node - no ghost bucket survives; got {:?}",
        concept_nodes(&g2)
    );
    // Every member realizes exactly the one surviving concept - the prior grain-1 edges were retired,
    // not left as stale duplicates.
    for member in ["docs/a.md", "src/a.rs", "docs/b.md", "src/b.rs"] {
        assert_eq!(
            live_realizes(&g2, member),
            ["concept/1/0".to_string()].into_iter().collect(),
            "{member} realizes exactly the one surviving concept after the fresh re-run"
        );
    }
}

#[test]
fn a_fresh_rerun_of_one_grain_leaves_every_other_resolution_grain_untouched() {
    // Per-grain scope of the `fresh` boundary: the supersession is keyed on the exact `concept/<r>/`
    // id prefix, so re-running r=1 must leave r=2's concept nodes and memberships entirely live. A
    // boundary that swept by KIND_CONCEPT alone (ignoring the grain prefix) would destroy the r=2
    // grain here.
    let p = Projector::open(":memory:", "test").unwrap();

    // r=1 grain and a coexisting r=2 grain.
    derived(&p, 1, "concept/1/0", 1.0, true);
    realized(&p, 2, "docs/a.md", "concept/1/0", 1.0);
    derived(&p, 3, "concept/2/0", 2.0, true);
    realized(&p, 4, "docs/a.md", "concept/2/0", 2.0);

    // Re-run ONLY the r=1 grain (its own fresh head).
    derived(&p, 5, "concept/1/0", 1.0, true);
    realized(&p, 6, "docs/a.md", "concept/1/0", 1.0);

    let g = p.whole().unwrap();
    assert!(
        g.nodes
            .iter()
            .any(|n| n.id == "concept/2/0" && n.kind == KIND_CONCEPT),
        "the r=2 concept node survives a r=1 re-run; got {:?}",
        concept_nodes(&g)
    );
    assert_eq!(
        live_realizes(&g, "docs/a.md"),
        ["concept/1/0".to_string(), "concept/2/0".to_string()]
            .into_iter()
            .collect(),
        "docs/a.md still realizes BOTH grains after the r=1-only re-run (the r=2 membership untouched)"
    );
}

#[test]
fn a_fresh_rerun_is_project_scoped_and_never_touches_another_projects_concepts() {
    // Spec 28: the `fresh` supersession's edge-retire and orphan-node-drop are BOTH scoped to
    // `self.project`. On a SHARED backend, two projects hold an identically-id'd `concept/1/0`; a
    // fresh re-run under project `px` must retire only px's REALIZES edge and drop only px's orphan
    // concept, leaving project `py`'s concept node and membership entirely live. A supersession that
    // forgot its `project = ?` clause would cross-contaminate here.
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("shared.db");
    let db = db.to_str().unwrap();

    let px = Projector::open(db, "px").unwrap();
    let py = Projector::open(db, "py").unwrap();

    // Project px: concept/1/0 = {docs/a.md}. Project py: an identically-id'd concept/1/0 = {docs/y.md}
    // plus a concept/1/1 = {docs/z.md} that py's grouping keeps.
    derived(&px, 1, "concept/1/0", 1.0, true);
    realized(&px, 2, "docs/a.md", "concept/1/0", 1.0);
    derived(&py, 3, "concept/1/0", 1.0, true);
    derived(&py, 4, "concept/1/1", 1.0, false);
    realized(&py, 5, "docs/y.md", "concept/1/0", 1.0);
    realized(&py, 6, "docs/z.md", "concept/1/1", 1.0);

    // Re-run px's r=1 grain: its fresh head retires px's grain-1 edges and drops px's orphan nodes.
    // py shares the id space but must be untouched.
    derived(&px, 7, "concept/1/0", 1.0, true);
    realized(&px, 8, "docs/a.md", "concept/1/0", 1.0);

    let gy = py.whole().unwrap();
    assert_eq!(
        concept_nodes(&gy),
        vec!["concept/1/0".to_string(), "concept/1/1".to_string()],
        "project py's concept nodes are untouched by project px's fresh re-run; got {:?}",
        concept_nodes(&gy)
    );
    assert_eq!(
        live_realizes(&gy, "docs/y.md"),
        ["concept/1/0".to_string()].into_iter().collect(),
        "project py's membership survives project px's fresh re-run"
    );
    assert_eq!(
        live_realizes(&gy, "docs/z.md"),
        ["concept/1/1".to_string()].into_iter().collect(),
        "project py's second concept membership survives project px's fresh re-run"
    );
    // And px itself re-materialized correctly (its single concept, one live member).
    let gx = px.whole().unwrap();
    assert_eq!(
        live_realizes(&gx, "docs/a.md"),
        ["concept/1/0".to_string()].into_iter().collect(),
        "project px re-materialized its own concept after the re-run"
    );
}
